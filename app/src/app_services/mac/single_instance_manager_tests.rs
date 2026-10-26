use std::path::Path;

use super::*;

#[test]
fn socket_path_is_absolute_in_state_dir() {
    let p = resolve_socket_path(Path::new("/Users/a/Library/State"), Path::new("/tmp"), "S");
    assert_eq!(p, Path::new("/Users/a/Library/State/S"));
    assert!(p.is_absolute());
}

#[test]
fn socket_path_falls_back_to_temp_when_state_dir_relative_or_too_long() {
    let p = resolve_socket_path(Path::new(""), Path::new("/var/T"), "S");
    assert_eq!(p, Path::new("/var/T/S"));
    let long = format!("/{}", "a".repeat(120));
    let p = resolve_socket_path(Path::new(&long), Path::new("/var/T"), "S");
    assert_eq!(p, Path::new("/var/T/S"));
}

#[test]
fn uri_socket_name_is_absolute() {
    assert!(Path::new(&uri_socket_name()).is_absolute());
}
