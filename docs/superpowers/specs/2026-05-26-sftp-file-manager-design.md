# SFTP File Manager Design Document

**Date**: 2026-05-26
**Status**: Approved

## Overview

Add a native SFTP file manager feature to the Zap terminal, using the `ssh2` crate (libssh2 bindings) to implement the SFTP protocol, providing full remote file browsing, transfer, and management capabilities. Implemented as an independent Pane panel, coexisting with the existing Server File Browser, with no remote daemon to install.

## Technical Approach

Implement the SFTP protocol directly with the `ssh2` crate, which has been validated as stable in comparable projects and has complete SFTP functionality (directory traversal, streaming transfer, permission management).

Dependencies: `ssh2` (libssh2 bindings), `smol` (async runtime), `thiserror` (error handling). On Windows, enable the `openssl-on-win32` feature with vendored openssl-sys.

## Crate Structure and Module Organization

### Protocol layer — `crates/warp_sftp/` (new crate)

```
crates/warp_sftp/
  Cargo.toml
  build.rs                          # Windows: link advapi32
  src/
    lib.rs                          # module root, exports the public API
    error.rs                        # SftpError / SftpChannelError
    types.rs                        # FileType / Metadata / DirEntry / OpenOptions, etc.
    session.rs                      # SftpSession (SSH connection management, auth)
    sftp.rs                         # Sftp (SFTP channel, file/directory operations)
    dir.rs                          # Dir (directory reading and sorting)
    file.rs                         # File (file read/write)
```

### UI layer — `app/src/sftp_manager/` (new module)

```
app/src/sftp_manager/
  mod.rs                            # module root
  types.rs                          # UI types: FileEntry / TransferTask / Dialog / ConnectionState
  sftp_ops.rs                       # high-level operation bridge
  browser.rs                        # SftpBrowserView main view
  file_list.rs                      # file list rendering
  breadcrumb.rs                     # breadcrumb navigation
  context_menu.rs                   # context menu
  dialogs.rs                        # dialogs
  transfer_panel.rs                 # transfer progress panel
```

### Pane integration

```
app/src/pane_group/pane/sftp_pane.rs (new)
```

## Core Protocol Layer Design

### session.rs — connection management

- `SftpSession`: internally holds `Arc<ssh2::Session>` + `TcpStream`
- `connect(host, port, username, auth_method) -> Result<SftpSession>`: establishes the TCP connection → SSH handshake → authentication
- `sftp() -> Result<Sftp>`: opens the SFTP subsystem on the existing session
- `disconnect()`: disconnects explicitly
- `Drop` disconnects automatically

`AuthMethod` enum: `Password(String)` | `PublicKey { path, passphrase }`

### sftp.rs — SFTP channel operations

- `Sftp`: wraps `Arc<Mutex<ssh2::Sftp>>`, Clone + thread-safe
- Operations: `open`, `create_dir`, `remove_dir`, `remove_file`, `rename`, `stat`, `lstat`, `read_dir`, `symlink`, `readlink`

### dir.rs — directory reading

- `Dir::read_dir() -> Result<Vec<DirEntry>>`
- Filters out `.` and `..`, converts to DirEntry
- Sorting: directories first, then alphabetical order

### file.rs — file read/write

- `File`: wraps `ssh2::File`
- Operations: `read_to_end`, `write_all`, `read` (32KB chunks), `write` (32KB chunks), `flush`, `stat`

### types.rs — core types

- `FileType`: Dir | File | Symlink | Other
- `FilePermissions`: 9-bit Unix permissions (rwxrwxrwx)
- `Metadata`: type, perms, size, uid, gid, atime, mtime
- `DirEntry`: name, path, metadata
- `OpenOptions`: read, write, append, create, truncate
- `WriteMode`: Overwrite | Append | Resume

### error.rs — error types

- `SftpError`: IO | SSH2 | ConnectionFailed | AuthFailed | Timeout | NoSuchFile | PermissionDenied | General
- `SftpChannelError`: Sftp | SendFailed | RecvFailed

## UI Layer Design

### browser.rs — SftpBrowserView main view

Implements the `BackingView` + `TypedActionView` + `View` traits.

**State**:

| Field | Type | Description |
|------|------|------|
| connection | ConnectionState | Connecting/Connected/Disconnected/Failed |
| _session | Option\<SftpSession\> | keeps the TCP connection alive |
| sftp | Option\<Sftp\> | SFTP channel |
| current_path | String | current directory path |
| entries | Vec\<FileEntry\> | file list for the current directory |
| selection | Option\<usize\> | selected item index |
| nav_history | NavHistory | back/forward history |
| transfers | Vec\<TransferTask\> | transfer queue |
| dialog | Option\<Dialog\> | current dialog state |
| search_filter | Option\<String\> | search filter |

**Action enum**:

- Connect(node_id), Disconnect
- NavigateTo(path), GoBack, GoForward, GoUp, Refresh
- Upload, Download, Delete, Rename, CreateFolder
- Select(index), Open(index)
- ShowContextMenu(index)
- CancelTransfer(task_id)
- Search(filter)

**Render structure** (top to bottom):

1. Toolbar: back/forward/up/refresh buttons + upload button + new-folder button
2. Breadcrumb navigation: clickable path segments
3. File list: table format (name/size/modified date), click to select, double-click to open
4. Transfer panel: collapsible at the bottom, showing active transfer tasks and progress
5. Context menu: open/download/rename/delete/details
6. Dialogs: delete confirmation, rename input, new-folder input, file details view

### sftp_ops.rs — high-level operation bridge

- `connect_from_server(server_info, secret_store) -> Result<(SftpSession, Sftp)>`: reads config from the SSH manager → obtains credentials → establishes the connection
- `list_dir(sftp, path) -> Result<Vec<FileEntry>>`
- `upload_file_streaming(sftp, local, remote, cancel_flag)`: 32KB chunks, AtomicBool supports cancellation
- `download_file_streaming(sftp, remote, local, cancel_flag)`: 32KB chunks
- `upload_dir_recursive`, `download_dir_recursive`
- `delete_file`, `delete_dir_recursive`, `create_dir`, `rename`
- Concurrency control: AtomicUsize CAS limits at most 2 parallel transfers

### Other UI modules

| Module | Responsibility |
|------|------|
| `file_list.rs` | file header + row rendering, directory/file icons, hover effects, selection highlight |
| `breadcrumb.rs` | clickable segments from root to the current path, each triggering NavigateTo |
| `context_menu.rs` | context menu items: open/download/rename/delete/details |
| `dialogs.rs` | modal dialogs, EditorView text input, Enter to confirm / Escape to cancel |
| `transfer_panel.rs` | transfer direction icon + filename + progress percentage + progress bar + status label |

### Keyboard shortcuts

| Key | Action |
|------|------|
| Backspace | go up to the parent directory |
| Delete | delete the selected item |
| Ctrl+Shift+N | create a new folder |
| Escape | cancel search / close dialog |

## Integration and Entry Points

### Integration with the SSH manager

The SFTP browser obtains connection info via `warp_ssh_manager`.

Entry points:

- `app/src/ssh_manager/panel.rs`: add a "Browse SFTP" option to the server context menu
- `app/src/ssh_manager/server_view.rs`: add a "Browse SFTP" button to the server detail action bar

Connection flow:

1. User right-clicks a server in the SSH host list → "Browse SFTP" menu item
2. Obtain SshServerInfo (host, port, username, auth_type, key_path)
3. Obtain the password/key passphrase via KeychainSecretStore
4. Build the AuthMethod
5. SftpOps::connect_from_server() establishes the connection
6. Open the SftpBrowserView Pane and display the root directory

### Pane system integration

- `app/src/pane_group/pane/sftp_pane.rs` (new): `SftpPane` wraps `SftpBrowserView` as `PaneContent`
  - Implements the PaneContent trait
  - Snapshot serializes to `LeafContents::Sftp { node_id }`
  - Automatically reconnects based on node_id when restored

Registration changes:

- `app/src/pane_group/pane/mod.rs`: declare the sftp_pane module
- `app/src/lib.rs`: register SftpPane with the View system

### Feature Flag

No feature flag is used; it's always available globally.

## Data Flow and Error Handling

### Operation data flow

```
User action (click/right-click/shortcut)
  → SftpBrowserView receives the Action
  → dispatch_typed_action() matches the Action type
  → an async task is submitted via ctx.spawn():
      ├── obtain the SftpOps / Sftp instance
      ├── perform the SFTP operation (running in the smol thread pool)
      └── return the result to the main thread
  → update SftpBrowserView state
  → trigger a re-render
```

### Connection lifecycle

```
Open Pane → Connect(node_id)
  → Connecting state (show a loading animation)
  → Success → Connected (load the root directory)
  → Failure → Failed (show an error message + retry button)

Close Pane → Drop
  → SftpSession disconnects automatically (Drop impl)
```

### Error handling strategy

| Scenario | Handling |
|------|----------|
| Connection failure (network/auth) | show an error message, offer a retry button, no popup |
| Directory load failure | show an error notice + refresh button in the file list area |
| File operation failure (delete/rename) | inline error notice (toast-style), doesn't block the UI |
| Transfer failure | transfer panel marked as Failed state, shows the failure reason |
| Connection dropped | automatically switches to Disconnected state, prompts to reconnect |

All errors are uniformly mapped through `SftpError` into user-readable Chinese-language messages.
