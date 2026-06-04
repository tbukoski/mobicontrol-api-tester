// Cross-platform default path resolution.
//
// On Windows: defaults under the user profile directory (e.g. C:\Users\<name>).
// On Linux/macOS: defaults under $HOME.

use std::path::PathBuf;

/// Returns the platform-appropriate default directory for storing user files.
/// Falls back to "." if the home directory cannot be determined.
pub fn default_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the suggested default path for the credentials file.
pub fn default_credentials_path() -> PathBuf {
    default_dir().join("mobicontrol_credentials.enc")
}

/// Returns the suggested default path for the API output file.
pub fn default_output_path() -> PathBuf {
    default_dir().join("mobicontrol_api_output.json")
}

/// Returns the path to the bundled sample swagger fallback (next to the executable).
/// If the executable path cannot be determined, falls back to the current working
/// directory.
pub fn sample_swagger_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("sample_swagger.json");
        }
    }
    PathBuf::from("sample_swagger.json")
}
