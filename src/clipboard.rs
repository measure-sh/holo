/// Returns `true` only when both opening the clipboard and writing the
/// text succeeded. Used by callers that want to surface success/failure
/// feedback to the user.
pub fn copy_to_clipboard(text: &str) -> bool {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    clipboard.set_text(text).is_ok()
}
