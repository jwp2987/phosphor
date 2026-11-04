use crate::elements::Fill;
use crate::geometry::vector::vec2f;
use crate::image_cache::StaticImage;
use crate::{
    elements::Point,
    fonts::{FontId, GlyphId},
    rendering,
};
use ordered_float::OrderedFloat;
use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use rstar::{RTree, primitives::Rectangle};
use std::sync::Arc;
use vec1::{Vec1, vec1};

/// See [`Scene::stats`]. Areas are device pixels, summed over primitives (overdraw counts).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneStats {
    pub layers: usize,
    pub clipped_layers: usize,
    pub rects: usize,
    /// Rects that draw nothing and are skipped by the renderer.
    pub skipped_rects: usize,
    pub rounded_rects: usize,
    pub gradient_rects: usize,
    pub bordered_rects: usize,
    /// Drawn rects of at least [`LARGE_RECT_AREA_PX`] device pixels.
    pub large_rects: usize,
    pub shadows: usize,
    pub glyphs: usize,
    pub images: usize,
    pub icons: usize,
    pub rect_area_px: f64,
    pub rounded_rect_area_px: f64,
    pub gradient_area_px: f64,
    /// Area of the shadow quads: the shader runs a 4-tap erf blur per pixel of these.
    pub shadow_area_px: f64,
    pub image_area_px: f64,
}

/// A rect of at least this many device pixels counts as "large" in [`SceneStats`].
const LARGE_RECT_AREA_PX: f64 = 250_000.;

#[derive(Clone)]
pub struct Scene {
    scale_factor: f32,
    rendering_config: rendering::Config,
    active_layer_index_stack: Vec1<ZIndex>,
    layers: Vec1<Layer>,
    overlay_layers: Vec<Layer>,
    /// Sizes the layers of this scene are pre-allocated to as they are started.
    capacity_hints: SceneCapacityHints,
    #[cfg(debug_assertions)]
    /// Custom panic location, set with [`Scene::set_location_for_panic_logging`]
    panic_location: Option<&'static std::panic::Location<'static>>,
}

#[derive(Clone, Default)]
pub struct Layer {
    hit_map: RTree<Rectangle<[OrderedFloat<f32>; 2]>>,
    pub clip_bounds: Option<RectF>,
    pub rects: Vec<Rect>,
    pub images: Vec<Image>,
    pub glyphs: Vec<Glyph>,
    pub icons: Vec<Icon>,
    pub click_through: bool,
}

/// Clip bounds to use for a layer.
pub enum ClipBounds {
    /// Use the bounds of the active layer.
    ActiveLayer,
    /// Use the specified bounds as the bounds for the new layer.
    ///
    /// Note that this ignores any clip bounds applied to the currently-active
    /// layer.
    BoundedBy(RectF),
    /// Intersect the active layer's bounds and the provided rect
    /// to get the bounds for the new layer.
    BoundedByActiveLayerAnd(RectF),
    /// No clipping
    None,
}

/// How many primitives of each kind a layer held. A frame's layers are usually the same shape as
/// the previous frame's (the same elements, with different content), so sizing a layer's vectors
/// up front from the last frame avoids the reallocations the vectors would otherwise go through as
/// they grow from empty on every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayerCapacity {
    rects: usize,
    glyphs: usize,
    images: usize,
    icons: usize,
}

impl LayerCapacity {
    fn of(layer: &Layer) -> Self {
        Self {
            rects: layer.rects.len(),
            glyphs: layer.glyphs.len(),
            images: layer.images.len(),
            icons: layer.icons.len(),
        }
    }
}

/// The [`LayerCapacity`] of every layer of a scene, in the order the layers were started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneCapacityHints {
    layers: Vec<LayerCapacity>,
    overlay_layers: Vec<LayerCapacity>,
}

impl SceneCapacityHints {
    fn normal(&self, index: usize) -> LayerCapacity {
        self.layers.get(index).copied().unwrap_or_default()
    }

    fn overlay(&self, index: usize) -> LayerCapacity {
        self.overlay_layers.get(index).copied().unwrap_or_default()
    }
}

impl Layer {
    fn with_capacity(clip_bounds: Option<RectF>, capacity: LayerCapacity) -> Self {
        Layer {
            clip_bounds,
            rects: Vec::with_capacity(capacity.rects),
            glyphs: Vec::with_capacity(capacity.glyphs),
            images: Vec::with_capacity(capacity.images),
            icons: Vec::with_capacity(capacity.icons),
            ..Default::default()
        }
    }

    fn record_hit_rect(&mut self, rect: RectF) {
        if let Some(intersected) = self
            .clip_bounds
            .map_or(Some(rect), |c| rect.intersection(c))
        {
            self.hit_map.insert(Rectangle::from_corners(
                [intersected.min_x().into(), intersected.min_y().into()],
                [intersected.max_x().into(), intersected.max_y().into()],
            ));
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct GlyphKey {
    pub glyph_id: GlyphId,
    pub font_id: FontId,
    pub font_size: OrderedFloat<f32>,
}

#[derive(Debug, Copy, Clone)]
pub enum GlyphFade {
    /// A horizontal fade from alpha 1 to 0 with start and end positions in screen coordinates
    /// start - where the fade is transparent
    /// end - where the fade is most opaque
    Horizontal { start: f32, end: f32 },
}

impl GlyphFade {
    pub fn horizontal(start: f32, end: f32) -> Self {
        GlyphFade::Horizontal { start, end }
    }
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub glyph_key: GlyphKey,
    pub position: Vector2F,
    pub fade: Option<GlyphFade>,
    pub color: ColorU,
}

#[derive(Clone, Default)]
pub struct Rect {
    pub bounds: RectF,
    pub drop_shadow: Option<DropShadow>,
    pub corner_radius: CornerRadius,
    pub background: Fill,
    pub border: Border,
}

impl Rect {
    /// Whether drawing this rect (its shadow aside) cannot change a pixel: the background has no
    /// alpha at either end and the border either is not drawn (no width on any side) or has none
    /// either. The rect shader returns alpha 0 for such a rect and the alpha blend leaves the
    /// destination as it was, so a renderer may skip it. Containers whose fill is `Fill::None`
    /// are the common case; each one costs a full pass over its bounds on a software rasterizer.
    pub fn draws_nothing(&self) -> bool {
        let transparent = |fill: &Fill| fill.start_color().a == 0 && fill.end_color().a == 0;
        let border_drawn = self.border.top_width() != 0.
            || self.border.right_width() != 0.
            || self.border.bottom_width() != 0.
            || self.border.left_width() != 0.;
        transparent(&self.background) && (!border_drawn || transparent(&self.border.color))
    }
}

#[derive(Clone)]
pub struct Image {
    pub bounds: RectF,
    pub asset: Arc<StaticImage>,
    pub opacity: f32,
    pub corner_radius: CornerRadius,
}

#[derive(Clone)]
pub struct Icon {
    pub bounds: RectF,
    pub asset: Arc<StaticImage>,
    pub opacity: f32,
    pub color: ColorU,
}

// These were picked empirically to make the shadows look decent by
// default, but there is nothing special about them.
const DEFAULT_DROP_SHADOW_OFFSET_X: f32 = 0.;
const DEFAULT_DROP_SHADOW_OFFSET_Y: f32 = 10.;
const DEFAULT_DROP_SHADOW_BLUR_RADIUS: f32 = 10.;
const DEFAULT_DROP_SHADOW_SPREAD_RADIUS: f32 = 30.;

#[derive(Clone, Copy)]
pub struct DropShadow {
    pub color: ColorU,

    // How the shadow is offset from the target rect
    pub offset: Vector2F,

    // Controls how tightly sampled the shadow is - the larger the number
    // the more spread out the shadow.
    pub blur_radius: f32,

    // Controls how wide the shadow is outside the target.
    pub spread_radius: f32,
}

impl DropShadow {
    pub fn new_with_standard_offset_and_spread(color: ColorU) -> Self {
        Self {
            color,
            offset: vec2f(DEFAULT_DROP_SHADOW_OFFSET_X, DEFAULT_DROP_SHADOW_OFFSET_Y),
            blur_radius: DEFAULT_DROP_SHADOW_BLUR_RADIUS,
            spread_radius: DEFAULT_DROP_SHADOW_SPREAD_RADIUS,
        }
    }

    pub fn with_offset(mut self, offset: Vector2F) -> Self {
        self.offset = offset;
        self
    }
}

impl Default for DropShadow {
    fn default() -> Self {
        Self::new_with_standard_offset_and_spread(ColorU::new(0, 0, 0, 32))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Fill,
    pub top: bool,
    pub left: bool,
    pub bottom: bool,
    pub right: bool,
    pub dash: Option<Dash>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Dash {
    pub dash_length: f32,
    pub gap_length: f32,

    /// If true, gaps will always be the length specified in `gap_length`.
    /// Otherwise, gap length may be adjusted slightly to guarantee that the
    /// dashed line starts and ends with a dash.
    pub force_consistent_gap_length: bool,
}

impl Border {
    pub fn top_width(&self) -> f32 {
        if self.top { self.width } else { 0.0 }
    }

    pub fn right_width(&self) -> f32 {
        if self.right { self.width } else { 0.0 }
    }

    pub fn bottom_width(&self) -> f32 {
        if self.bottom { self.width } else { 0.0 }
    }

    pub fn left_width(&self) -> f32 {
        if self.left { self.width } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Radius {
    /// Specify a radius in absolute pixels.
    Pixels(f32),
    /// Specify a radius as a percentage of the rectangle's smaller dimension.
    /// For example, using `Percentage(50.)` will produce a pill shape.
    Percentage(f32),
}

impl Default for Radius {
    fn default() -> Self {
        Radius::Pixels(0.)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadius {
    /// Top left corner radius
    top_left: Option<Radius>,
    /// Top right corner radius
    top_right: Option<Radius>,
    /// Bottom left corner radius
    bottom_left: Option<Radius>,
    /// Bottom right corner radius
    bottom_right: Option<Radius>,
}

impl CornerRadius {
    /// Whether every corner is square: unset, or an absolute radius of zero. `with_all(Pixels(0.))`
    /// draws the same as `CornerRadius::default()`, but compares unequal to it.
    pub fn is_square(&self) -> bool {
        [
            self.top_left,
            self.top_right,
            self.bottom_left,
            self.bottom_right,
        ]
        .into_iter()
        .all(|radius| match radius {
            None => true,
            Some(Radius::Pixels(pixels)) => pixels == 0.,
            Some(Radius::Percentage(_)) => false,
        })
    }

    /// Merge this CornerRadius struct with another.
    /// `Some(r)` takes precedence over `None`.
    /// If both are present, `other`'s values, take precedence over `self`'s existing values.
    pub fn merge(&mut self, other: CornerRadius) {
        self.top_left = other.top_left.or(self.top_left);
        self.top_right = other.top_right.or(self.top_right);
        self.bottom_left = other.bottom_left.or(self.bottom_left);
        self.bottom_right = other.bottom_right.or(self.bottom_right);
    }

    pub fn get_top_left(&self) -> Radius {
        self.top_left.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_top_right(&self) -> Radius {
        self.top_right.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_bottom_left(&self) -> Radius {
        self.bottom_left.unwrap_or(Radius::Pixels(0.))
    }

    pub fn get_bottom_right(&self) -> Radius {
        self.bottom_right.unwrap_or(Radius::Pixels(0.))
    }

    pub const fn with_all(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }
    pub const fn with_top(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_bottom(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }
    pub const fn with_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: None,
        }
    }
    pub const fn with_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: Some(radius),
        }
    }
    pub const fn with_top_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: Some(radius),
            top_right: None,
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_top_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: Some(radius),
            bottom_left: None,
            bottom_right: None,
        }
    }
    pub const fn with_bottom_left(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: Some(radius),
            bottom_right: None,
        }
    }
    pub const fn with_bottom_right(radius: Radius) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: None,
            bottom_right: Some(radius),
        }
    }

    /// Filters this [`CornerRadius`] to only have the top corners rounded.
    pub const fn top(self) -> Self {
        CornerRadius {
            top_left: self.top_left,
            top_right: self.top_right,
            bottom_left: None,
            bottom_right: None,
        }
    }

    /// Filters this [`CornerRadius`] to only have the bottom corners rounded.
    pub const fn bottom(self) -> Self {
        CornerRadius {
            top_left: None,
            top_right: None,
            bottom_left: self.bottom_left,
            bottom_right: self.bottom_right,
        }
    }
}

/// Where in a layer the next primitive would be recorded. Taken with [`Scene::insertion_point`]
/// while painting; [`Scene::splice_at`] later inserts primitives painted elsewhere there, so they
/// end up ordered exactly as if they had been painted at that moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerInsertionPoint {
    z_index: ZIndex,
    rects: usize,
    images: usize,
    glyphs: usize,
    icons: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Newtype to encapsulate a Z index, which actually represents a layer index in the list of layers
pub enum ZIndex {
    Normal(usize),
    Overlay(usize),
}

impl ZIndex {
    #[cfg(test)]
    pub fn new(layer: usize) -> Self {
        ZIndex::Normal(layer)
    }
}

impl Scene {
    pub fn new(scale_factor: f32, rendering_config: rendering::Config) -> Self {
        Self::with_capacity_hints(
            scale_factor,
            rendering_config,
            SceneCapacityHints::default(),
        )
    }

    /// A scene whose layers are pre-allocated to the sizes in `capacity_hints`, normally the
    /// [`Self::capacity_hints`] of the previous frame's scene. Only the allocation differs: the
    /// primitives recorded, and their order, are the same as with [`Self::new`].
    pub fn with_capacity_hints(
        scale_factor: f32,
        rendering_config: rendering::Config,
        capacity_hints: SceneCapacityHints,
    ) -> Self {
        Self {
            scale_factor,
            rendering_config,
            active_layer_index_stack: vec1![ZIndex::Normal(0)],
            layers: vec1![Layer::with_capacity(None, capacity_hints.normal(0))],
            overlay_layers: Vec::new(),
            capacity_hints,
            #[cfg(debug_assertions)]
            panic_location: None,
        }
    }

    /// The sizes of this scene's layers, to pre-allocate the next frame's scene with.
    pub fn capacity_hints(&self) -> SceneCapacityHints {
        SceneCapacityHints {
            layers: self.layers.iter().map(LayerCapacity::of).collect(),
            overlay_layers: self.overlay_layers.iter().map(LayerCapacity::of).collect(),
        }
    }

    /// Temporarily set the panic location for the scene. This is cleared
    /// during the next draw call.
    #[cfg(debug_assertions)]
    pub fn set_location_for_panic_logging(
        &mut self,
        panic_location: Option<&'static std::panic::Location<'static>>,
    ) {
        self.panic_location = panic_location;
    }

    fn active_layer(&mut self) -> &mut Layer {
        match *self.active_layer_index_stack.last() {
            ZIndex::Normal(index) => &mut self.layers[index],
            ZIndex::Overlay(index) => &mut self.overlay_layers[index],
        }
    }

    pub fn is_covered(&self, position: Point) -> bool {
        // Does any layer at a higher z-index contain this point?
        let point = [position.x().into(), position.y().into()];
        let predicate = |l: &Layer| !l.click_through && l.hit_map.locate_at_point(&point).is_some();

        match position.z_index() {
            ZIndex::Normal(index) => self
                .layers
                .get((index + 1)..)
                .into_iter()
                .flatten()
                .chain(self.overlay_layers.iter())
                .any(predicate),
            ZIndex::Overlay(index) => self
                .overlay_layers
                .get((index + 1)..)
                .into_iter()
                .flatten()
                .any(predicate),
        }
    }

    // Compute the intersection between the bound of the element and the clip bound
    // on its current layer. The intersection is then checked against the event position
    // to determine whether we should dispatch the event.
    pub fn visible_rect(&self, origin: Point, size: Vector2F) -> Option<RectF> {
        // TODO: investigate under what conditions a z-index that doesn't exist in
        // the scene gets passed in.
        // Historical crash samples show this is fairly common but doesn't much
        // affect normal usage; may occur during a view teardown frame.
        let maybe_layer = match origin.z_index() {
            ZIndex::Normal(index) => self.layers.get(index),
            ZIndex::Overlay(index) => self.overlay_layers.get(index),
        };
        let maybe_bounds = maybe_layer.and_then(|layer| layer.clip_bounds);

        let input_rect = RectF::new(origin.xy(), size);
        match maybe_bounds {
            Some(clip_rect) => clip_rect.intersection(input_rect),
            None => Some(input_rect),
        }
    }

    /// Get the Z-Index of the currently-active layer
    pub fn z_index(&self) -> ZIndex {
        *self.active_layer_index_stack.last()
    }

    /// Get the maximum Z-Index in the active layer stack (whether Normal or Overlay).
    pub fn max_active_z_index(&self) -> ZIndex {
        match self.active_layer_index_stack.last() {
            ZIndex::Normal(_) => ZIndex::Normal(self.layers.len() - 1),
            // Safety: If the active layer is an overlay layer, then there must be at least one
            // overlay layer, so subtracting one from the length is valid.
            ZIndex::Overlay(_) => ZIndex::Overlay(self.overlay_layers.len() - 1),
        }
    }

    pub fn start_layer(&mut self, bounds: ClipBounds) {
        let clip_bounds = self.clip_bounds_for_new_layer(bounds);

        match *self.active_layer_index_stack.last() {
            ZIndex::Normal(_) => self.push_normal_layer(clip_bounds),
            ZIndex::Overlay(_) => self.push_overlay_layer(clip_bounds),
        }
    }

    pub(crate) fn start_overlay_layer(&mut self, bounds: ClipBounds) {
        let clip_bounds = self.clip_bounds_for_new_layer(bounds);
        self.push_overlay_layer(clip_bounds);
    }

    fn clip_bounds_for_new_layer(&mut self, bounds: ClipBounds) -> Option<RectF> {
        match bounds {
            ClipBounds::ActiveLayer => self.active_layer().clip_bounds,
            ClipBounds::BoundedBy(bounds) => Some(bounds),
            ClipBounds::BoundedByActiveLayerAnd(bounds) => {
                if let Some(current_layer_bounds) = self.active_layer().clip_bounds {
                    // If the current layer has bounds, return the intersection...
                    current_layer_bounds
                        .intersection(bounds)
                        // ...or, if the regions don't overlap, an empty bounding rect.
                        .or(Some(RectF::default()))
                } else {
                    // If the current layer has no bounds, return the bounds
                    // for the new layer.
                    Some(bounds)
                }
            }
            ClipBounds::None => None,
        }
    }

    fn push_normal_layer(&mut self, clip_bounds: Option<RectF>) {
        let index = self.layers.len();
        self.active_layer_index_stack.push(ZIndex::Normal(index));
        self.layers.push(Layer::with_capacity(
            clip_bounds,
            self.capacity_hints.normal(index),
        ));
    }

    fn push_overlay_layer(&mut self, clip_bounds: Option<RectF>) {
        let index = self.overlay_layers.len();
        self.active_layer_index_stack.push(ZIndex::Overlay(index));
        self.overlay_layers.push(Layer::with_capacity(
            clip_bounds,
            self.capacity_hints.overlay(index),
        ));
    }

    pub fn set_active_layer_click_through(&mut self) {
        self.active_layer().click_through = true;
    }

    pub fn stop_layer(&mut self) {
        if self.active_layer_index_stack.pop().is_err() {
            panic!("popped the last layer from active_layer_index_stack");
        }
    }

    fn validate_rect(
        rect: &RectF,
        #[cfg_attr(not(debug_assertions), allow(unused_variables))] location: Option<
            &'static std::panic::Location<'static>,
        >,
    ) {
        #[cfg(debug_assertions)]
        let location_info = location
            .map(|loc| {
                format!(
                    " (element created at {}:{}:{})",
                    loc.file(),
                    loc.line(),
                    loc.column()
                )
            })
            .unwrap_or_default();
        #[cfg(not(debug_assertions))]
        let location_info = "";
        debug_assert!(
            !rect.origin().y().is_infinite(),
            "!rect.origin().y().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.origin().y().is_nan(),
            "!rect.origin().y().is_nan(){location_info}"
        );

        debug_assert!(
            !rect.size().x().is_infinite(),
            "!rect.size().x().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.size().x().is_nan(),
            "!rect.size().x().is_nan(){location_info}"
        );
        debug_assert!(
            !rect.size().y().is_infinite(),
            "!rect.size().y().is_infinite(){location_info}"
        );
        debug_assert!(
            !rect.size().y().is_nan(),
            "!rect.size().y().is_nan(){location_info}"
        );
    }

    /// This method draws a rectangle without recording any information about it in the current
    /// layer. Note this should be used with caution. In most cases, what you need is
    /// `draw_rect_with_hit_recording` instead. However, in rare cases this may be useful for
    /// performance reasons when many intermediate rects are drawn. If this is called, it is up to
    /// the caller to also draw a rect (via draw_rect_with_hit_recording) that encompasses the range
    /// of the rects drawn so that layer recording for event dispatching is correctly kept
    /// up-to-date.
    pub fn draw_rect_without_hit_recording(&mut self, rect: RectF) -> &mut Rect {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.rects.push(Rect {
            bounds: rect,
            ..Default::default()
        });
        layer.rects.last_mut().unwrap()
    }

    pub fn draw_rect_with_hit_recording(&mut self, rect: RectF) -> &mut Rect {
        let layer = self.active_layer();
        layer.record_hit_rect(rect);
        self.draw_rect_without_hit_recording(rect)
    }

    pub fn draw_image(
        &mut self,
        rect: RectF,
        asset: Arc<StaticImage>,
        opacity: f32,
        corner_radius: CornerRadius,
    ) {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.images.push(Image {
            bounds: rect,
            asset,
            opacity,
            corner_radius,
        });
        layer.record_hit_rect(rect);
    }

    pub fn draw_icon(&mut self, rect: RectF, asset: Arc<StaticImage>, opacity: f32, color: ColorU) {
        #[cfg(debug_assertions)]
        let location = self.panic_location.take();
        #[cfg(not(debug_assertions))]
        let location = None;
        let layer = self.active_layer();
        Self::validate_rect(&rect, location);

        layer.icons.push(Icon {
            bounds: rect,
            asset,
            opacity,
            color,
        });
        layer.record_hit_rect(rect);
    }

    /// Adds a glyph that should be drawn in the scene.
    ///
    /// `position` is the point at which the glyph's left edge meets the
    /// baseline.
    pub fn draw_glyph(
        &mut self,
        position: Vector2F,
        glyph_id: GlyphId,
        font_id: FontId,
        font_size: f32,
        color: ColorU,
    ) -> &mut Glyph {
        // TODO: Support hit testing on glyphs?
        let layer = self.active_layer();
        layer.glyphs.push(Glyph {
            glyph_key: GlyphKey {
                glyph_id,
                font_id,
                font_size: font_size.into(),
            },
            position,
            color,
            fade: None,
        });
        layer.glyphs.last_mut().unwrap()
    }

    /// An empty scene with this scene's scale and rendering config, to paint primitives into
    /// that are later inserted with [`Self::splice_at`].
    pub fn scratch(&self) -> Scene {
        Scene::new(self.scale_factor, self.rendering_config)
    }

    /// Where in the active layer the next primitive would go.
    pub fn insertion_point(&mut self) -> LayerInsertionPoint {
        let z_index = self.z_index();
        let layer = self.active_layer();
        LayerInsertionPoint {
            z_index,
            rects: layer.rects.len(),
            images: layer.images.len(),
            glyphs: layer.glyphs.len(),
            icons: layer.icons.len(),
        }
    }

    /// Inserts the base layer of `painted` (a [`Self::scratch`] scene) at `point`, as if its
    /// primitives had been recorded there. An insertion moves every point recorded later in the
    /// same layer, so splice in reverse order of recording. Layers `painted` started beyond its
    /// base layer are not carried over.
    pub fn splice_at(&mut self, point: LayerInsertionPoint, painted: Scene) {
        let Scene {
            layers,
            overlay_layers,
            ..
        } = painted;
        let mut layers = layers.into_vec();
        debug_assert!(
            layers.len() == 1 && overlay_layers.is_empty(),
            "a spliced scene must not start layers of its own"
        );
        let base = layers.swap_remove(0);
        let layer = match point.z_index {
            ZIndex::Normal(index) => &mut self.layers[index],
            ZIndex::Overlay(index) => &mut self.overlay_layers[index],
        };
        drop(layer.rects.splice(point.rects..point.rects, base.rects));
        drop(layer.images.splice(point.images..point.images, base.images));
        drop(layer.glyphs.splice(point.glyphs..point.glyphs, base.glyphs));
        drop(layer.icons.splice(point.icons..point.icons, base.icons));
        for hit_rect in base.hit_map.iter() {
            layer.hit_map.insert(*hit_rect);
        }
    }

    /// Get an iterator over all layers in order, from bottom to top
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().chain(self.overlay_layers.iter())
    }

    /// Get the total number of layers
    #[cfg(test)]
    pub fn layer_count(&self) -> usize {
        self.layers.len() + self.overlay_layers.len()
    }

    /// Per-kind primitive counts and covered device-pixel areas of the scene, for the
    /// `warpui::scene_stats` debug log (#1279). Walks the layers once; only called when that log
    /// target is enabled.
    pub fn stats(&self) -> SceneStats {
        let sf = self.scale_factor;
        let area = |bounds: RectF, clip: Option<RectF>| -> f64 {
            let b = match clip {
                Some(c) => bounds.intersection(c),
                None => Some(bounds),
            };
            b.map_or(0., |b| (b.width() * sf) as f64 * (b.height() * sf) as f64)
        };
        let mut stats = SceneStats::default();
        for layer in self.layers() {
            stats.layers += 1;
            if layer.clip_bounds.is_some() {
                stats.clipped_layers += 1;
            }
            let clip = layer.clip_bounds;
            for rect in &layer.rects {
                if let Some(shadow) = rect.drop_shadow {
                    stats.shadows += 1;
                    let pad = shadow.spread_radius;
                    let b = RectF::new(
                        rect.bounds.origin() + shadow.offset - vec2f(pad, pad),
                        rect.bounds.size() + vec2f(2. * pad, 2. * pad),
                    );
                    stats.shadow_area_px += area(b, clip);
                }
                if rect.draws_nothing() {
                    stats.skipped_rects += 1;
                    continue;
                }
                stats.rects += 1;
                let a = area(rect.bounds, clip);
                stats.rect_area_px += a;
                if !rect.corner_radius.is_square() {
                    stats.rounded_rects += 1;
                    stats.rounded_rect_area_px += a;
                }
                if matches!(rect.background, Fill::Gradient { .. }) {
                    stats.gradient_rects += 1;
                    stats.gradient_area_px += a;
                }
                if rect.border.top_width() != 0.
                    || rect.border.right_width() != 0.
                    || rect.border.bottom_width() != 0.
                    || rect.border.left_width() != 0.
                {
                    stats.bordered_rects += 1;
                }
                if a >= LARGE_RECT_AREA_PX {
                    stats.large_rects += 1;
                }
            }
            stats.glyphs += layer.glyphs.len();
            stats.images += layer.images.len();
            stats.icons += layer.icons.len();
            for i in &layer.images {
                stats.image_area_px += area(i.bounds, clip);
            }
            for i in &layer.icons {
                stats.image_area_px += area(i.bounds, clip);
            }
        }
        stats
    }

    /// A hash of everything the renderer would draw (rects, shadows, glyphs, images, icons, layer
    /// clips). Two frames with the same fingerprint put the same pixels on screen, so logging it
    /// beside [`Scene::stats`] shows which frames repainted identical content (#1279, a second
    /// frame after every key). Debug-log use only.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let f = |h: &mut std::collections::hash_map::DefaultHasher, v: f32| v.to_bits().hash(h);
        let rect = |h: &mut std::collections::hash_map::DefaultHasher, r: RectF| {
            f(h, r.origin_x());
            f(h, r.origin_y());
            f(h, r.width());
            f(h, r.height());
        };
        let fill = |h: &mut std::collections::hash_map::DefaultHasher, fill: &Fill| {
            for c in [fill.start_color(), fill.end_color()] {
                (c.r, c.g, c.b, c.a).hash(h);
            }
            f(h, fill.start().x());
            f(h, fill.start().y());
            f(h, fill.end().x());
            f(h, fill.end().y());
        };
        self.scale_factor.to_bits().hash(&mut h);
        for layer in self.layers() {
            layer.clip_bounds.is_some().hash(&mut h);
            if let Some(c) = layer.clip_bounds {
                rect(&mut h, c);
            }
            for r in &layer.rects {
                rect(&mut h, r.bounds);
                fill(&mut h, &r.background);
                fill(&mut h, &r.border.color);
                for w in [
                    r.border.top_width(),
                    r.border.right_width(),
                    r.border.bottom_width(),
                    r.border.left_width(),
                ] {
                    f(&mut h, w);
                }
                format!("{:?}", r.corner_radius).hash(&mut h);
                if let Some(d) = r.drop_shadow {
                    (d.color.r, d.color.g, d.color.b, d.color.a).hash(&mut h);
                    f(&mut h, d.offset.x());
                    f(&mut h, d.offset.y());
                    f(&mut h, d.blur_radius);
                    f(&mut h, d.spread_radius);
                }
            }
            for g in &layer.glyphs {
                g.glyph_key.glyph_id.hash(&mut h);
                f(&mut h, g.position.x());
                f(&mut h, g.position.y());
                (g.color.r, g.color.g, g.color.b, g.color.a).hash(&mut h);
                format!("{:?}", g.fade).hash(&mut h);
            }
            for i in &layer.images {
                rect(&mut h, i.bounds);
                Arc::as_ptr(&i.asset).hash(&mut h);
                f(&mut h, i.opacity);
            }
            for i in &layer.icons {
                rect(&mut h, i.bounds);
                Arc::as_ptr(&i.asset).hash(&mut h);
                f(&mut h, i.opacity);
                (i.color.r, i.color.g, i.color.b, i.color.a).hash(&mut h);
            }
        }
        h.finish()
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn rendering_config(&self) -> &rendering::Config {
        &self.rendering_config
    }
}

impl Rect {
    pub fn with_corner_radius(&mut self, radius: CornerRadius) -> &mut Self {
        self.corner_radius.merge(radius);
        self
    }

    pub fn with_border(&mut self, border: Border) -> &mut Self {
        self.border = border;
        self
    }

    pub fn with_background<F>(&mut self, background: F) -> &mut Self
    where
        F: Into<Fill>,
    {
        self.background = background.into();
        self
    }

    pub fn with_drop_shadow(&mut self, drop_shadow: DropShadow) -> &mut Self {
        self.drop_shadow = Some(drop_shadow);
        self
    }
}

impl Glyph {
    pub fn with_fade(&mut self, fade: Option<GlyphFade>) -> &mut Self {
        self.fade = fade;
        self
    }
}

#[cfg(test)]
#[path = "scene_test.rs"]
mod tests;
