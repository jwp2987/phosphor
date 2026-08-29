use crate::util::extensions::TrimStringExt;

#[test]
fn test_trim_newline() {
    let mut string = "".to_string();
    string.trim_trailing_newline();
    assert_eq!("", string);

    let mut string = "test\n".to_string();
    string.trim_trailing_newline();
    assert_eq!("test", string);

    let mut string = "   test    \n".to_string();
    string.trim_trailing_newline();
    assert_eq!("   test    ", string);
}
