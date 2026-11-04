//! Reading one JSON registry file into its type.

/// Load one stage file; a missing file is an empty registry (tolerated), a
/// corrupt one is a structured error string.
pub fn load_stage<T: serde::de::DeserializeOwned + Default>(
    path: &std::path::Path,
) -> Result<T, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s)
            .map_err(|e| format!("{}: unreadable stage file: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
