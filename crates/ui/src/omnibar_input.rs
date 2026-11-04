//! Borrowed preflight before copying clipboard/IME text into the omnibar.
pub const LIMIT: usize = 4096;
pub const TOO_LONG: &str = "Entrada demasiado larga (máximo 4096 bytes)";
#[cfg(test)]
mod tests {
    use super::*;
    fn append(text: &mut String, input: &str, replace: bool) -> Result<(), &'static str> {
        let mut cursor = crate::line_edit::Cursor::default();
        cursor.select(text, text.len(), text.len());
        cursor.insert(text, input, replace)
    }
    #[test]
    fn rejected_paste_keeps_text_and_capacity_in_both_modes() {
        let paste = "x".repeat(1024 * 1024);
        let mut text = "keep".to_string();
        let capacity = text.capacity();
        for replace in [false, true] {
            assert_eq!(append(&mut text, &paste, replace), Err(TOO_LONG));
            assert_eq!(text, "keep");
            assert_eq!(text.capacity(), capacity);
        }
    }
    #[test]
    fn unicode_exact_limit_append_and_filtered_bytes() {
        let mut s = String::new();
        append(&mut s, &"á".repeat(2048), false).unwrap();
        assert_eq!(s.len(), LIMIT);
        assert!(append(&mut s, "x", false).is_err());
        assert_eq!(s.len(), LIMIT);
        append(&mut s, "\n日本\r語\0", true).unwrap();
        assert_eq!(s, "日本語");
        append(&mut s, "😀", false).unwrap();
        assert_eq!(s, "日本語😀");
    }
}
