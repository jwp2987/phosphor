use crate::cloud_object::{
    GenericStoredObject, GenericStringObjectFormat, JsonObjectType, ObjectIdType, ObjectType,
    Owner, Revision, Space, StoredObjectLocation, StoredObjectModel,
};
use crate::drive::folders::{FolderObject, FolderObjectModel};
use crate::drive::{DriveIndexVariant, ObjectTypeAndId};
use crate::env_vars::{EnvVarCollection, EnvVarCollectionObject};
use crate::notebooks::NotebookObject;
use crate::persistence::ModelEvent;
use crate::server::ids::{HashableId, ObjectUid, SyncId, ToServerId};
use crate::workflows::WorkflowObject;
use crate::workflows::workflow::Workflow;
use crate::workflows::workflow_enum::WorkflowEnumObject;

use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::mpsc::SyncSender;

use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::cloud_object::StoredObject;
use crate::util::sync::Condition;
use chrono::{DateTime, Utc};
use rand::Rng;
use warp_core::features::FeatureFlag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSource {
    /// This object store change came from an external overwrite path.
    External,
    /// This object store change originated locally (i.e. from a user edit).
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectStoreEvent {
    ObjectMoved {
        type_and_id: ObjectTypeAndId,
        source: UpdateSource,
        from_folder: Option<SyncId>,
        to_folder: Option<SyncId>,
    },
    ObjectUpdated {
        type_and_id: ObjectTypeAndId,
        source: UpdateSource,
    },
    ObjectTrashed {
        type_and_id: ObjectTypeAndId,
        source: UpdateSource,
    },
    ObjectUntrashed {
        type_and_id: ObjectTypeAndId,
        source: UpdateSource,
    },
    NotebookEditorChangedExternally {
        notebook_id: SyncId,
    },
    ObjectCreated {
        type_and_id: ObjectTypeAndId,
    },
    /// An object was permanently deleted.
    ObjectDeleted {
        type_and_id: ObjectTypeAndId,
        /// The parent folder of this object, since it's no longer in the model.
        folder_id: Option<SyncId>,
    },
    /// An object's permissioned were changed.
    ObjectPermissionsUpdated {
        type_and_id: ObjectTypeAndId,
        source: UpdateSource,
    },
    /// An object identified by `id` was force expanded.
    ObjectForceExpanded {
        id: String,
    },
    /// The initial bulk load of object store entries has completed.
    InitialLoadCompleted,
}

enum FolderOpenState {
    Open,
    Closed,
    Reversed,
}

/// The persistence model for [StoredObject] information. In Zap it
/// corresponds to the local object store within SQLite.
/// Logic beyond basic update/query should live in [ObjectStoreViewModel] and be covered in model_test.rs.
pub struct ObjectStoreModel {
    objects_by_id: HashMap<ObjectUid, Box<dyn StoredObject>>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
    initial_load_complete: Condition,

    time_of_next_force_refresh: Option<DateTime<Utc>>,
}

impl ObjectStoreModel {
    pub fn new(
        model_event_sender: Option<SyncSender<ModelEvent>>,
        cached_objects: Vec<Box<dyn StoredObject>>,
        time_of_next_force_refresh: Option<DateTime<Utc>>,
    ) -> Self {
        let objects_by_id = cached_objects
            .into_iter()
            .map(|object| (object.uid().to_owned(), object))
            .collect::<HashMap<ObjectUid, Box<dyn StoredObject>>>();
        let initial_load_complete = Condition::new();
        // Zap has no initial cloud object fetch; it can be considered readable as soon as SQLite restore finishes.
        initial_load_complete.set();

        Self {
            objects_by_id,
            model_event_sender,
            initial_load_complete,
            time_of_next_force_refresh,
        }
    }

    /// Waits for the local object store to become readable. Under Zap this condition is satisfied immediately after SQLite restore.
    pub fn initial_load_complete(&self) -> impl Future<Output = ()> + use<> {
        self.initial_load_complete.wait()
    }

    /// Determines whether or not the given object_id can be moved to the given location, based on
    /// what we currently support from an API perspective.
    ///
    /// We do NOT support
    /// - Moving folders across spaces
    /// - Transferring from team space to personal space
    /// - Moving directly into a folder across spaces
    pub fn can_move_object_to_location(
        &self,
        hashed_id: &str,
        new_location: StoredObjectLocation,
        app: &AppContext,
    ) -> bool {
        // TODO(ben): Update as sharing+moving is supported in more cases.

        if let Some(object) = self.objects_by_id.get(hashed_id) {
            let object_space = object.space(app);
            if let StoredObjectLocation::Space(space) = new_location {
                if matches!(object_space, Space::Team { .. }) && space == Space::Personal {
                    return false;
                }

                if !object.can_move_to_space(space, app) {
                    return false;
                }
            }

            if let StoredObjectLocation::Folder(target_folder_id) = new_location {
                let folder_to_move: Option<&FolderObject> = object.into();
                if let Some(folder_to_move) = folder_to_move {
                    // We do not want to move a folder into itself.
                    if folder_to_move.id == target_folder_id {
                        return false;
                    }

                    // Since we are trying to move a folder into a folder, we want to ensure that the
                    // target folder is not a child of the folder we are trying to move.
                    let mut target_folder_parent_folder_id = self
                        .get_folder(&target_folder_id)
                        .and_then(|folder| folder.metadata().folder_id);
                    while let Some(parent_id) = target_folder_parent_folder_id {
                        if parent_id == folder_to_move.id {
                            return false;
                        }
                        target_folder_parent_folder_id = self
                            .get_folder(&parent_id)
                            .and_then(|folder| folder.metadata().folder_id);
                    }
                }
                if let Some(target_folder) = self.get_folder(&target_folder_id) {
                    // TODO: @ianhodge We do not yet support moving directly into a folder from another space
                    if target_folder.permissions.owner != object.permissions().owner {
                        return false;
                    }
                }
            }

            return true;
        }
        false
    }

    /// Given a hashed object-id, returns the object's StoredObjectLocation
    /// (either a folder or top level space)
    pub fn object_location(
        &self,
        hashed_id: &str,
        app: &AppContext,
    ) -> Option<StoredObjectLocation> {
        self.objects_by_id
            .get(hashed_id)
            .map(|object| object.location(self, app))
    }

    pub fn get_by_uid(&self, uid: &ObjectUid) -> Option<&dyn StoredObject> {
        self.objects_by_id.get(uid).map(|o| o.as_ref())
    }

    pub fn get_mut_by_uid(&mut self, uid: &ObjectUid) -> Option<&mut Box<dyn StoredObject>> {
        self.objects_by_id.get_mut(uid)
    }

    pub fn cloud_objects(&self) -> impl Iterator<Item = &Box<dyn StoredObject>> {
        self.objects_by_id.values()
    }

    pub fn create_object(
        &mut self,
        id: SyncId,
        object: impl StoredObject + 'static,
        ctx: &mut ModelContext<ObjectStoreModel>,
    ) {
        ctx.emit(ObjectStoreEvent::ObjectCreated {
            type_and_id: object.object_type_and_id(),
        });
        self.create_object_internal(id, object);
        ctx.notify();
    }

    // Does not emit events or notify — used during initial load where
    // InitialLoadCompleted is emitted once afterward instead.
    fn create_object_internal(&mut self, id: SyncId, object: impl StoredObject + 'static) {
        self.objects_by_id.insert(id.uid(), Box::new(object));
    }

    pub fn delete_objects_by_id(
        &mut self,
        uids: Vec<ObjectUid>,
        ctx: &mut ModelContext<Self>,
    ) -> (Vec<(SyncId, ObjectIdType)>, i32) {
        let mut count = 0;
        let mut sync_ids_and_types: Vec<(SyncId, ObjectIdType)> = Vec::new();
        for uid in uids {
            if let Some(object) = self.objects_by_id.remove(&uid) {
                let object_type_and_id = object.object_type_and_id();
                sync_ids_and_types.push((
                    object_type_and_id.sync_id(),
                    object_type_and_id.object_id_type(),
                ));

                ctx.emit(ObjectStoreEvent::ObjectDeleted {
                    type_and_id: object.object_type_and_id(),
                    folder_id: object.metadata().folder_id,
                });
                count += 1;
            }
        }
        ctx.notify();
        (sync_ids_and_types, count)
    }

    /// Update an object's location (folder and owner). This is an implementation detail of
    /// `UpdateManager` to keep local state in sync with optimistic moves. It does not validate
    /// that the move is valid and MUST not be used elsewhere.
    ///
    /// If `new_space` is `None`, the space is unchanged.
    pub fn update_object_location(
        &mut self,
        uid: &ObjectUid,
        new_owner: Option<Owner>,
        new_folder: Option<SyncId>,
        ctx: &mut ModelContext<Self>,
    ) {
        // TODO(ben): This should use a container instead of a folder+owner pair.

        if let Some(object) = self.get_mut_by_uid(uid) {
            let old_folder = object.metadata().folder_id;
            let mut changed = false;

            if let Some(new_owner) = new_owner {
                if new_owner != object.permissions().owner {
                    object.permissions_mut().owner = new_owner;
                    changed = true;
                }
            }

            if new_folder != old_folder {
                object.metadata_mut().folder_id = new_folder;
                changed = true;
            }

            if changed {
                ctx.emit(ObjectStoreEvent::ObjectMoved {
                    type_and_id: object.object_type_and_id(),
                    source: UpdateSource::Local,
                    from_folder: old_folder,
                    to_folder: new_folder,
                });
                ctx.notify();
            }
        }
    }

    /// Update an object in the object store as part of a local user edit.
    pub fn update_object_from_edit<K, M>(
        &mut self,
        model: M,
        object_id: SyncId,
        ctx: &mut ModelContext<Self>,
    ) where
        K: HashableId
            + ToServerId
            + std::fmt::Debug
            + Into<String>
            + Clone
            + Copy
            + Send
            + Sync
            + 'static,
        M: StoredObjectModel<IdType = K, StoredObjectType = GenericStoredObject<K, M>> + 'static,
    {
        if let Some(cloud_object) = self.get_object_of_type_mut(&object_id) {
            cloud_object.set_model(model);
            ctx.emit(ObjectStoreEvent::ObjectUpdated {
                type_and_id: cloud_object.object_type_and_id(),
                source: UpdateSource::Local,
            });
            ctx.notify();
        }
    }

    fn set_folder_open_state(
        &mut self,
        folder_id: SyncId,
        open_state: FolderOpenState,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(folder) = self.get_folder_mut(&folder_id) {
            let is_open = match open_state {
                FolderOpenState::Open => true,
                FolderOpenState::Closed => false,
                FolderOpenState::Reversed => !folder.model.is_open,
            };

            folder.set_model(FolderObjectModel {
                is_open,
                is_warp_pack: folder.model.is_warp_pack,
                name: folder.model.name.clone(),
            });

            let folder_clone = folder.clone();
            if let Some(model_event_sender) = &self.model_event_sender {
                if let Err(e) = model_event_sender.send(folder_clone.upsert_event()) {
                    log::error!("Error persisting folder: {e:?}");
                }
            }

            ctx.notify();
        }
    }

    pub fn open_folder(&mut self, folder_id: SyncId, ctx: &mut ModelContext<Self>) {
        self.set_folder_open_state(folder_id, FolderOpenState::Open, ctx)
    }

    pub fn close_folder(&mut self, folder_id: SyncId, ctx: &mut ModelContext<Self>) {
        self.set_folder_open_state(folder_id, FolderOpenState::Closed, ctx)
    }

    pub fn toggle_folder_open(&mut self, folder_id: SyncId, ctx: &mut ModelContext<Self>) {
        self.set_folder_open_state(folder_id, FolderOpenState::Reversed, ctx)
    }

    /// Collapses all folders for a given location, including the folder provided
    /// (if location is a StoredObjectLocation::Folder).
    pub fn collapse_all_in_location(
        &mut self,
        location: StoredObjectLocation,
        index_variant: DriveIndexVariant,
        ctx: &mut ModelContext<Self>,
    ) {
        let mut folder_ids: Vec<SyncId> = Vec::new();
        self.collapse_all_in_location_helper(location, index_variant, &mut folder_ids, ctx);

        folder_ids.iter().for_each(|folder_id| {
            self.set_folder_open_state(*folder_id, FolderOpenState::Closed, ctx)
        });

        ctx.notify();
    }

    /// Helper function for collapse_all_in_location. Recursively traverses through descendents,
    /// adding IDs of any folders found to the folder_ids mutable vector reference.
    fn collapse_all_in_location_helper(
        &self,
        location: StoredObjectLocation,
        index_variant: DriveIndexVariant,
        folder_ids: &mut Vec<SyncId>,
        app: &AppContext,
    ) {
        if let StoredObjectLocation::Folder(folder_id) = location {
            folder_ids.push(folder_id);
        }

        match index_variant {
            DriveIndexVariant::MainIndex => self
                .active_cloud_objects_in_location_without_descendents(location, app)
                .for_each(|object| {
                    let folder: Option<&FolderObject> = object.into();
                    if let Some(folder) = folder {
                        self.collapse_all_in_location_helper(
                            StoredObjectLocation::Folder(folder.id),
                            index_variant,
                            folder_ids,
                            app,
                        );
                    }
                }),
            DriveIndexVariant::Trash => {
                if let StoredObjectLocation::Space(space) = location {
                    self.directly_trashed_cloud_objects_in_space(space, app)
                        .for_each(|object| {
                            let folder: Option<&FolderObject> = object.into();
                            if let Some(folder) = folder {
                                self.collapse_all_in_location_helper(
                                    StoredObjectLocation::Folder(folder.id),
                                    index_variant,
                                    folder_ids,
                                    app,
                                );
                            }
                        })
                } else {
                    self.indirectly_trashed_cloud_objects_in_location_without_descendents(
                        location, app,
                    )
                    .for_each(|object| {
                        let folder: Option<&FolderObject> = object.into();
                        if let Some(folder) = folder {
                            self.collapse_all_in_location_helper(
                                StoredObjectLocation::Folder(folder.id),
                                index_variant,
                                folder_ids,
                                app,
                            );
                        }
                    })
                }
            }
        }
    }

    /// Force expands the object identified by `hash_id` and any of its ancestors. If an object is
    /// identified by `id`, [`ObjectStoreEvent::ObjectForceExpanded`] is emitted.
    pub fn force_expand_object_and_ancestors(&mut self, id: SyncId, ctx: &mut ModelContext<Self>) {
        let hashed_id = &id.uid();
        if !self.objects_by_id.contains_key(hashed_id) {
            return;
        }

        self.force_expand_object_and_ancestors_internal(id, ctx);
        ctx.emit(ObjectStoreEvent::ObjectForceExpanded {
            id: hashed_id.clone(),
        });
    }

    fn force_expand_object_and_ancestors_internal(
        &mut self,
        id: SyncId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(object) = self.objects_by_id.get(&id.uid()) else {
            return;
        };

        let parent_folder_id = object.metadata().folder_id;
        let folder: Option<&FolderObject> = object.into();

        if let Some(folder) = folder {
            self.set_folder_open_state(folder.id, FolderOpenState::Open, ctx);
        }

        if let Some(parent_folder_id) = parent_folder_id {
            self.force_expand_object_and_ancestors_internal(parent_folder_id, ctx);
        }
    }

    /// Force expands object and its ancestors when given a ObjectTypeAndId input
    pub fn force_expand_object_and_ancestors_cloud_id(
        &mut self,
        id: ObjectTypeAndId,
        ctx: &mut ModelContext<Self>,
    ) {
        match id {
            ObjectTypeAndId::Notebook(sync_id) => {
                self.force_expand_object_and_ancestors(sync_id, ctx)
            }
            ObjectTypeAndId::Workflow(sync_id) => {
                self.force_expand_object_and_ancestors(sync_id, ctx)
            }
            ObjectTypeAndId::Folder(sync_id) => {
                self.force_expand_object_and_ancestors(sync_id, ctx)
            }
            ObjectTypeAndId::GenericStringObject { object_type, id } => {
                if let GenericStringObjectFormat::Json(JsonObjectType::EnvVarCollection) =
                    object_type
                {
                    self.force_expand_object_and_ancestors(id, ctx)
                } else {
                    log::error!("Attempted to force expand an unsupported GenericStringObject type")
                }
            }
        }
    }

    pub fn delete_object(&mut self, id: SyncId, ctx: &mut ModelContext<Self>) {
        // TODO: for now we are simply hard deleting the object from memory. When
        // we have conflict resolution. We should only mark the object as deleted
        // without deleting the content until the server returns successful response.
        if let Some(object) = self.objects_by_id.remove(&id.uid()) {
            ctx.emit(ObjectStoreEvent::ObjectDeleted {
                type_and_id: object.object_type_and_id(),
                folder_id: object.metadata().folder_id,
            });
        }
        ctx.notify();
    }

    /// The number of local objects that have entered an error state and will be shown in the Zap Drive index.
    pub fn num_visible_errored_objects(&self) -> usize {
        self.objects_by_id
            .values()
            .filter(|object| object.renders_in_warp_drive() && object.metadata().is_errored())
            .count()
    }

    pub fn has_non_welcome_objects(&self) -> bool {
        self.objects_by_id
            .iter()
            .any(|(_, object)| !object.metadata().is_welcome_object)
    }

    pub fn get_folder_by_uid(&self, uid: &str) -> Option<&FolderObject> {
        self.objects_by_id.get(uid).and_then(|object| object.into())
    }

    pub fn get_folder(&self, folder_id: &SyncId) -> Option<&FolderObject> {
        self.objects_by_id
            .get(&folder_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_folder_mut(&mut self, folder_id: &SyncId) -> Option<&mut FolderObject> {
        self.objects_by_id
            .get_mut(&folder_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_all_exportable_object_ids(&self) -> Vec<ObjectTypeAndId> {
        self.objects_by_id
            .values()
            .filter(|object| object.can_export())
            .map(|object| object.object_type_and_id())
            .collect()
    }

    /// Returns all folders (trashed or not) in object store.
    #[cfg(test)]
    pub fn get_all_active_and_inactive_folders(&self) -> impl Iterator<Item = &FolderObject> {
        self.objects_by_id
            .values()
            .filter_map(|object| object.into())
    }

    pub fn get_workflow(&self, workflow_id: &SyncId) -> Option<&WorkflowObject> {
        self.objects_by_id
            .get(&workflow_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_workflow_by_uid(&self, uid: &str) -> Option<&WorkflowObject> {
        self.objects_by_id.get(uid).and_then(|object| object.into())
    }

    pub fn get_workflow_enum(&self, enum_id: &SyncId) -> Option<&WorkflowEnumObject> {
        self.objects_by_id
            .get(&enum_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_workflow_mut(&mut self, workflow_id: &SyncId) -> Option<&mut WorkflowObject> {
        self.objects_by_id
            .get_mut(&workflow_id.uid())
            .and_then(|object| object.into())
    }

    /// Returns only active (not trashed) workflows in object store.
    pub fn get_all_active_workflows(&self) -> impl Iterator<Item = &WorkflowObject> {
        self.objects_by_id
            .values()
            .filter(|object| !object.is_trashed(self))
            .filter_map(|object| object.into())
    }

    /// Returns all workflows (trashed or not) in object store.
    pub fn get_all_active_and_inactive_workflows(&self) -> impl Iterator<Item = &WorkflowObject> {
        self.objects_by_id
            .values()
            .filter_map(|object| object.into())
    }

    /// Returns all active (not trashed) workflows in the space.
    pub fn active_workflows_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a WorkflowObject> + 'a {
        self.active_cloud_objects_in_space(space, app)
            .filter_map(|object| object.into())
    }

    /// Returns all active (not trashed) and non-welcome workflows (ie. non starter workflows) in the space.
    pub fn active_non_welcome_workflows_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a WorkflowObject> + 'a {
        self.active_non_welcome_cloud_objects_in_space(space, app)
            .filter_map(|object| object.into())
    }

    /// Returns all active (not trashed) notebooks in the space.
    pub fn active_notebooks_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a NotebookObject> + 'a {
        self.active_cloud_objects_in_space(space, app)
            .filter_map(|object| object.into())
    }

    /// Returns all active (not trashed) and non-welcome notebooks (ie. non starter notebooks) in the space.
    pub fn active_non_welcome_notebooks_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a NotebookObject> + 'a {
        self.active_non_welcome_cloud_objects_in_space(space, app)
            .filter_map(|object| object.into())
    }

    /// Returns all active (not trashed) and non-welcome env var collections in the space.
    pub fn active_non_welcome_env_var_collections_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a EnvVarCollectionObject> + 'a {
        self.active_non_welcome_cloud_objects_in_space(space, app)
            .filter_map(|object| object.into())
    }

    /// Returns all workflow enums with a given owner.
    pub fn workflow_enums_with_owner<'a>(
        &'a self,
        owner: Owner,
        _: &'a AppContext,
    ) -> impl Iterator<Item = &'a WorkflowEnumObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| !object.is_trashed(self) && object.permissions().owner == owner)
            .filter_map(|object| object.into())
    }

    pub fn get_object_of_type<K, M>(&self, object_id: &SyncId) -> Option<&GenericStoredObject<K, M>>
    where
        K: HashableId + ToServerId + std::fmt::Debug + Into<String> + Clone + 'static,
        M: StoredObjectModel<IdType = K, StoredObjectType = GenericStoredObject<K, M>> + 'static,
    {
        self.objects_by_id
            .get(&object_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_object_of_type_mut<K, M>(
        &mut self,
        object_id: &SyncId,
    ) -> Option<&mut GenericStoredObject<K, M>>
    where
        K: HashableId + ToServerId + std::fmt::Debug + Into<String> + Clone + 'static,
        M: StoredObjectModel<IdType = K, StoredObjectType = GenericStoredObject<K, M>> + 'static,
    {
        self.objects_by_id
            .get_mut(&object_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_all_objects_of_type<K, M>(&self) -> impl Iterator<Item = &GenericStoredObject<K, M>>
    where
        K: HashableId + ToServerId + std::fmt::Debug + Into<String> + Clone + 'static,
        M: StoredObjectModel<IdType = K, StoredObjectType = GenericStoredObject<K, M>> + 'static,
    {
        self.objects_by_id
            .values()
            .filter_map(|object| object.into())
    }

    pub fn get_notebook(&self, notebook_id: &SyncId) -> Option<&NotebookObject> {
        self.objects_by_id
            .get(&notebook_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_notebook_by_uid(&self, uid: &str) -> Option<&NotebookObject> {
        self.objects_by_id.get(uid).and_then(|object| object.into())
    }

    pub fn get_notebook_mut(&mut self, notebook_id: &SyncId) -> Option<&mut NotebookObject> {
        self.objects_by_id
            .get_mut(&notebook_id.uid())
            .and_then(|notebook| notebook.into())
    }

    pub fn get_env_var_collection(
        &self,
        env_var_collection_id: &SyncId,
    ) -> Option<&EnvVarCollectionObject> {
        self.objects_by_id
            .get(&env_var_collection_id.uid())
            .and_then(|object| object.into())
    }

    pub fn get_env_var_collection_by_uid(&self, uid: &str) -> Option<&EnvVarCollectionObject> {
        self.objects_by_id.get(uid).and_then(|object| object.into())
    }

    /// Returns only active (not trashed) EVCs in object store.
    pub fn get_all_active_env_var_collections(
        &self,
    ) -> impl Iterator<Item = &EnvVarCollectionObject> {
        self.objects_by_id
            .values()
            .filter(|object| !object.is_trashed(self))
            .filter_map(|object| object.into())
    }

    pub fn current_revision(&self, id: &SyncId) -> Option<&Revision> {
        self.objects_by_id
            .get(&id.uid())
            .and_then(|warp_cloud_object| warp_cloud_object.metadata().revision.as_ref())
    }

    /// Returns only active (not trashed) notebooks in object store.
    pub fn get_all_active_notebooks(&self) -> impl Iterator<Item = &NotebookObject> {
        self.objects_by_id
            .values()
            .filter(|object| !object.is_trashed(self))
            .filter_map(|object| object.into())
    }

    /// Returns all notebooks (trashed or not) in object store.
    pub fn get_all_active_and_inactive_notebooks(&self) -> impl Iterator<Item = &NotebookObject> {
        self.objects_by_id
            .values()
            .filter_map(|object| object.into())
    }

    #[cfg(test)]
    pub fn as_cloud_objects(&self) -> impl Iterator<Item = &'_ Box<dyn StoredObject>> {
        self.objects_by_id.values()
    }

    #[cfg(test)]
    pub fn add_object(&mut self, id: SyncId, object: impl StoredObject + 'static) {
        self.objects_by_id.insert(id.uid(), Box::new(object));
    }

    /// Pre-computes the set of UIDs for all active (non-trashed) objects using memoization.
    /// This is O(N) amortized instead of O(N × D) for the naive approach, because each
    /// object's trashed status is computed at most once and cached.
    pub fn active_object_uids(&self) -> HashSet<ObjectUid> {
        let mut cache = HashMap::new();
        let mut visiting = HashSet::new();
        let mut active = HashSet::new();
        for uid in self.objects_by_id.keys() {
            if !self.is_trashed_memoized(uid, &mut cache, &mut visiting) {
                active.insert(uid.clone());
            }
        }
        active
    }

    /// Memoized version of `is_trashed` that caches results to avoid redundant ancestor traversals.
    fn is_trashed_memoized(
        &self,
        uid: &str,
        cache: &mut HashMap<String, bool>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if let Some(&cached) = cache.get(uid) {
            return cached;
        }

        // Cycle detection: if we're already visiting this UID in the current traversal, treat as trashed.
        if visiting.contains(uid) {
            return true;
        }

        let result = match self.objects_by_id.get(uid) {
            Some(object) => {
                if object.metadata().trashed_ts.is_some() {
                    true
                } else {
                    match object.metadata().folder_id.map(|parent_id| parent_id.uid()) {
                        Some(parent_uid) => {
                            visiting.insert(uid.to_owned());
                            let r = self.is_trashed_memoized(&parent_uid, cache, visiting);
                            visiting.remove(uid);
                            r
                        }
                        None => false,
                    }
                }
            }
            None => !FeatureFlag::SharedWithMe.is_enabled(),
        };

        cache.insert(uid.to_owned(), result);
        result
    }

    /// Given a StoredObjectLocation (folder or space), returns local objects
    /// located directly in that location that are not trashed. This function
    /// does not recurse into nested folders.
    pub fn active_cloud_objects_in_location_without_descendents<'a>(
        &'a self,
        location: StoredObjectLocation,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| {
                !object.is_trashed(self) && object.location(self, app) == location
            })
            .map(|object| object.as_ref())
    }

    /// Given a StoredObjectLocation (folder or space), returns local objects
    /// located directly in that location that are trashed. This function does
    /// not recurse into nested folders.
    pub fn trashed_cloud_objects_in_location_without_descendents<'a>(
        &'a self,
        location: StoredObjectLocation,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| object.is_trashed(self) && object.location(self, app) == location)
            .map(|object| object.as_ref())
    }

    pub fn trashed_cloud_object_types_in_location_with_descendants(
        &self,
        location: StoredObjectLocation,
        app: &AppContext,
    ) -> Vec<ObjectType> {
        let mut trashed_objects: Vec<ObjectType> = Vec::new();
        self.trashed_cloud_object_types_in_location_with_descendants_helper(
            location,
            &mut trashed_objects,
            app,
        );
        trashed_objects
    }

    /// Helper function for trashed_cloud_objects_in_location_with_descendants.
    /// Recursively traverses through descendants, adding object types of any trashed
    /// objects found to the trashed_objects mutable vector reference.
    fn trashed_cloud_object_types_in_location_with_descendants_helper(
        &self,
        location: StoredObjectLocation,
        trashed_objects: &mut Vec<ObjectType>,
        app: &AppContext,
    ) {
        // Fetch direct descendants of the location
        self.trashed_cloud_objects_in_location_without_descendents(location, app)
            .for_each(|object| {
                trashed_objects.push(object.object_type());
                let folder: Option<&FolderObject> = object.into();
                // If any of the direct descendants are folders, recursively traverse through them
                if let Some(folder) = folder {
                    self.trashed_cloud_object_types_in_location_with_descendants_helper(
                        StoredObjectLocation::Folder(folder.id),
                        trashed_objects,
                        app,
                    );
                }
            });
    }

    /// Given a StoredObjectLocation (folder or space), returns local objects
    /// located directly in that location that entered the trash because a
    /// parent was trashed, but were not explicitly trashed by the user
    /// themselves. This function does not recurse into nested folders.
    pub fn indirectly_trashed_cloud_objects_in_location_without_descendents<'a>(
        &'a self,
        location: StoredObjectLocation,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> {
        self.objects_by_id
            .values()
            .filter(move |object| {
                object.is_trashed(self)
                    && object.location(self, app) == location
                    && object.metadata().trashed_ts.is_none()
            })
            .map(|object| object.as_ref())
    }

    /// Returns all active (not trashed) local objects in the given space.
    pub fn active_cloud_objects_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| object.is_in_space(space, app) && !object.is_trashed(self))
            .map(|object| object.as_ref())
    }

    /// Returns all active (not trashed) and non-welcome local objects in the given space.
    pub fn active_non_welcome_cloud_objects_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| {
                object.is_in_space(space, app)
                    && !object.is_trashed(self)
                    && !object.is_welcome_object()
            })
            .map(|object| object.as_ref())
    }

    // Returns all objects, trashed or otherwise, in the space.
    pub fn all_cloud_objects_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| object.is_in_space(space, app))
            .map(|object| object.as_ref())
    }

    /// Returns all trashed local objects in the given space.
    pub fn trashed_cloud_objects_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> + 'a {
        self.objects_by_id
            .values()
            .filter(move |object| object.is_in_space(space, app) && object.is_trashed(self))
            .map(|object| object.as_ref())
    }

    /// Returns all local objects in the given space that were explicitly trashed by the user.
    pub fn directly_trashed_cloud_objects_in_space<'a>(
        &'a self,
        space: Space,
        app: &'a AppContext,
    ) -> impl Iterator<Item = &'a dyn StoredObject> {
        self.objects_by_id
            .values()
            .filter(move |object| {
                object.is_in_space(space, app) && object.metadata().trashed_ts.is_some()
            })
            .map(|object| object.as_ref())
    }

    /// Returns a map of how many active (not trashed) objects reside within specified spaces.
    pub fn num_active_cloud_objects_per_space<'a, I>(
        &self,
        spaces: I,
        app: &AppContext,
    ) -> HashMap<Space, usize>
    where
        I: Iterator<Item = &'a Space>,
    {
        spaces
            .map(|space| {
                (
                    *space,
                    self.active_cloud_objects_in_space(*space, app).count(),
                )
            })
            .collect::<HashMap<_, _>>()
    }

    /// Returns a map of how many trashed objects reside within specified spaces.
    pub fn num_trashed_cloud_objects_per_space<'a, I>(
        &self,
        spaces: I,
        app: &AppContext,
    ) -> HashMap<Space, usize>
    where
        I: Iterator<Item = &'a Space>,
    {
        spaces
            .map(|space| {
                (
                    *space,
                    self.trashed_cloud_objects_in_space(*space, app).count(),
                )
            })
            .collect::<HashMap<_, _>>()
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn mock(_ctx: &mut ModelContext<Self>) -> Self {
        Self::new(None, Vec::new(), None)
    }

    // If the object is a folder and a welcome object, open it if we haven't opened a welcome folder before.

    pub fn reset(&mut self) {
        self.objects_by_id = HashMap::new();
        self.initial_load_complete.set();
        self.time_of_next_force_refresh = None;
    }
}

impl Entity for ObjectStoreModel {
    type Event = ObjectStoreEvent;
}

/// Mark ObjectStoreModel as global application state.
impl SingletonEntity for ObjectStoreModel {}

#[cfg(test)]
#[path = "model_test.rs"]
mod tests;
