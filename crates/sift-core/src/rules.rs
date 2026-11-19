//! Cleanup knowledge as data. Add a rule here and both the scanner and the
//! suggestions pick it up.

use crate::model::Safety;

/// A folder that a tool rebuilds automatically, matched by name.
pub struct RegenRule {
    pub id: &'static str,
    pub dir_name: &'static str,
    /// Where the folder must sit for the rule to apply.
    pub context: Context,
    pub label: &'static str,
    pub how: &'static str,
    pub safety: Safety,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Any,
    /// Parent has `Assets/` and `ProjectSettings/`.
    Unity,
    /// Parent has a `*.uproject` file.
    Unreal,
    /// Parent has a `Cargo.toml` file.
    Cargo,
}

pub const REGEN_RULES: &[RegenRule] = &[
    RegenRule { id: "node_modules", dir_name: "node_modules", context: Context::Any, label: "npm/yarn/pnpm dependencies", how: "Quarantine for dormant projects; `npm install` restores them.", safety: Safety::Review },
    RegenRule { id: "next_cache", dir_name: ".next", context: Context::Any, label: "Next.js build cache", how: "Rebuilt on the next `npm run dev` or `build`.", safety: Safety::Safe },
    RegenRule { id: "turbo_cache", dir_name: ".turbo", context: Context::Any, label: "Turborepo cache", how: "Rebuilt automatically.", safety: Safety::Safe },
    RegenRule { id: "parcel_cache", dir_name: ".parcel-cache", context: Context::Any, label: "Parcel cache", how: "Rebuilt automatically.", safety: Safety::Safe },
    RegenRule { id: "angular_cache", dir_name: ".angular", context: Context::Any, label: "Angular build cache", how: "Rebuilt automatically.", safety: Safety::Safe },
    RegenRule { id: "pycache", dir_name: "__pycache__", context: Context::Any, label: "Python bytecode", how: "Regenerated when the code runs.", safety: Safety::Safe },
    RegenRule { id: "pytest_cache", dir_name: ".pytest_cache", context: Context::Any, label: "pytest cache", how: "Regenerated on the next test run.", safety: Safety::Safe },
    RegenRule { id: "venv", dir_name: ".venv", context: Context::Any, label: "Python virtualenv", how: "Recreate with `pip install -r requirements.txt`.", safety: Safety::Review },
    RegenRule { id: "venv2", dir_name: "venv", context: Context::Any, label: "Python virtualenv (probably)", how: "Check it holds pyvenv.cfg; recreate with pip.", safety: Safety::Review },
    RegenRule { id: "cargo_target", dir_name: "target", context: Context::Cargo, label: "Rust build output (target/)", how: "Rebuilt by the next `cargo build` (a full rebuild takes a while). Same as `cargo clean`.", safety: Safety::Safe },
    RegenRule { id: "gradle_project", dir_name: ".gradle", context: Context::Any, label: "Gradle project cache", how: "Rebuilt on the next build.", safety: Safety::Safe },
    RegenRule { id: "unity_library", dir_name: "Library", context: Context::Unity, label: "Unity Library (import cache)", how: "Close Unity first. Reimports on next open (slow for big projects).", safety: Safety::Review },
    RegenRule { id: "unity_temp", dir_name: "Temp", context: Context::Unity, label: "Unity Temp", how: "Close Unity first.", safety: Safety::Safe },
    RegenRule { id: "unity_obj", dir_name: "obj", context: Context::Unity, label: "Unity C# build intermediates", how: "Close Unity first.", safety: Safety::Safe },
    RegenRule { id: "unity_logs", dir_name: "Logs", context: Context::Unity, label: "Unity logs", how: "Safe to remove.", safety: Safety::Safe },
    RegenRule { id: "unity_builds", dir_name: "Builds", context: Context::Unity, label: "Unity build output (probably)", how: "Remove old builds you've shipped or archived.", safety: Safety::Review },
    RegenRule { id: "unreal_ddc", dir_name: "DerivedDataCache", context: Context::Unreal, label: "Unreal DerivedDataCache", how: "Close the editor first. Rebuilt on next open (shader compiles take a while).", safety: Safety::Safe },
    RegenRule { id: "unreal_intermediate", dir_name: "Intermediate", context: Context::Unreal, label: "Unreal Intermediate build files", how: "Close the editor first. Regenerated on the next build.", safety: Safety::Safe },
    RegenRule { id: "unreal_binaries", dir_name: "Binaries", context: Context::Unreal, label: "Unreal compiled binaries", how: "Close the editor first. Rebuilt from source (C++ projects only — Blueprint-only projects don't need it).", safety: Safety::Review },
    RegenRule { id: "unreal_saved", dir_name: "Saved", context: Context::Unreal, label: "Unreal Saved (logs, autosaves, crash reports)", how: "Check first: holds autosaves and local config as well as logs.", safety: Safety::Review },
];

pub fn regen_rule(id: &str) -> Option<&'static RegenRule> {
    REGEN_RULES.iter().find(|r| r.id == id)
}

/// Extensions whose totals become their own suggestion.
pub struct SpecialExt {
    pub id: &'static str,
    pub exts: &'static [&'static str],
    pub label: &'static str,
    pub how: &'static str,
    pub safety: Safety,
}

pub const SPECIAL_EXTS: &[SpecialExt] = &[
    SpecialExt { id: "blend_backup", exts: &["blend1", "blend2", "blend3"], label: "Blender backup files (.blend1/.blend2)", how: "Auto-backups saved next to each .blend. Remove for finished projects; Preferences → Save & Load → Save Versions = 0 stops them.", safety: Safety::Review },
    SpecialExt { id: "dumps", exts: &["dmp"], label: "Crash dump files", how: "Only useful for debugging a crash.", safety: Safety::Safe },
];

pub fn special_for_ext(ext: &str) -> Option<&'static str> {
    SPECIAL_EXTS.iter().find(|s| s.exts.contains(&ext)).map(|s| s.id)
}

pub fn category(ext: &str) -> &'static str {
    match ext {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v" | "wmv" | "flv" | "ts" => "Video",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" => "Audio",
        "jpg" | "jpeg" | "png" | "gif" | "psd" | "tif" | "tiff" | "raw" | "cr2" | "nef" | "arw" | "heic" | "webp" | "exr" | "hdr" | "kra" | "ai" => "Images & design",
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" => "Archives",
        "exe" | "msi" | "iso" | "img" | "msix" | "msixbundle" | "appx" | "dmg" => "Installers & disk images",
        "vhdx" | "vhd" | "vmdk" | "qcow2" | "vdi" => "Virtual disks",
        "blend" | "blend1" | "blend2" | "fbx" | "obj" | "glb" | "gltf" | "unitypackage" | "uasset" | "umap" | "pak" | "abc" | "usd" | "usdz" => "3D & game assets",
        "pdf" | "docx" | "doc" | "pptx" | "xlsx" | "epub" | "txt" | "md" => "Documents",
        "dll" | "pdb" | "lib" | "so" | "a" | "jar" | "whl" | "node" | "pyd" | "ilk" | "pch" => "Dev binaries",
        "log" | "dmp" | "tmp" | "etl" | "bak" | "old" => "Logs, dumps & temp",
        _ => "Other",
    }
}

/// A fixed location worth checking. `paths` may contain `*` globs (one level)
/// and `%VAR%` environment references.
pub struct KnownLocation {
    pub id: &'static str,
    pub label: &'static str,
    pub paths: &'static [&'static str],
    pub safety: Safety,
    pub how: &'static str,
    pub action: KnownAction,
}

pub enum KnownAction {
    Quarantine,
    Command(&'static str, &'static [&'static str]),
    Settings(&'static str),
    Manual,
}

#[cfg(windows)]
pub const KNOWN_LOCATIONS: &[KnownLocation] = &[
    KnownLocation { id: "user_temp", label: "User temp files", paths: &["%TEMP%"], safety: Safety::Safe, how: "Temp files apps forgot to clean. Files in use are skipped.", action: KnownAction::Quarantine },
    KnownLocation { id: "win_temp", label: "Windows temp", paths: &["%SystemRoot%\\Temp"], safety: Safety::Safe, how: "Needs Sift running as admin.", action: KnownAction::Quarantine },
    KnownLocation { id: "wu_cache", label: "Windows Update cache", paths: &["%SystemRoot%\\SoftwareDistribution\\Download"], safety: Safety::Safe, how: "Settings → Storage → Temporary files → Windows Update Cleanup.", action: KnownAction::Settings("ms-settings:storagepolicies") },
    KnownLocation { id: "do_cache", label: "Delivery Optimization cache", paths: &["%SystemRoot%\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"], safety: Safety::Safe, how: "Settings → Storage → Temporary files → Delivery Optimization Files.", action: KnownAction::Settings("ms-settings:storagepolicies") },
    KnownLocation { id: "windows_old", label: "Previous Windows installation", paths: &["%SystemDrive%\\Windows.old"], safety: Safety::Safe, how: "Settings → Storage → Temporary files. You lose the option to roll back.", action: KnownAction::Settings("ms-settings:storagepolicies") },
    KnownLocation { id: "recycle_bin", label: "Recycle Bin", paths: &["%SystemDrive%\\$Recycle.Bin"], safety: Safety::Safe, how: "Empty the Recycle Bin.", action: KnownAction::Manual },
    KnownLocation { id: "crash_dumps", label: "Crash dumps & kernel reports", paths: &["%LOCALAPPDATA%\\CrashDumps", "%SystemRoot%\\Minidump", "%SystemRoot%\\MEMORY.DMP", "%SystemRoot%\\LiveKernelReports"], safety: Safety::Safe, how: "Only useful when debugging a crash.", action: KnownAction::Quarantine },
    KnownLocation { id: "shader_cache", label: "DirectX / GPU shader caches", paths: &["%LOCALAPPDATA%\\D3DSCache", "%LOCALAPPDATA%\\NVIDIA\\DXCache", "%LOCALAPPDATA%\\NVIDIA\\GLCache", "%LOCALAPPDATA%\\AMD\\DxCache", "%LOCALAPPDATA%\\AMD\\DxcCache"], safety: Safety::Safe, how: "Rebuilt automatically; first launch of each game stutters briefly.", action: KnownAction::Quarantine },
    KnownLocation { id: "browser_cache", label: "Browser caches", paths: &["%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Cache", "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Code Cache", "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Cache", "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Code Cache", "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\*\\Cache", "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\*\\Code Cache", "%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\cache2"], safety: Safety::Safe, how: "Close the browser first, or clear 'Cached images and files' in its settings.", action: KnownAction::Quarantine },
    KnownLocation { id: "app_caches", label: "Discord / Spotify / Teams caches", paths: &["%APPDATA%\\discord\\Cache", "%APPDATA%\\discord\\Code Cache", "%LOCALAPPDATA%\\Spotify\\Data", "%APPDATA%\\Microsoft\\Teams\\Cache"], safety: Safety::Safe, how: "Close the app first.", action: KnownAction::Quarantine },
    KnownLocation { id: "vscode_cache", label: "VS Code caches", paths: &["%APPDATA%\\Code\\Cache", "%APPDATA%\\Code\\CachedData", "%APPDATA%\\Code\\CachedExtensionVSIXs", "%APPDATA%\\Code\\User\\workspaceStorage"], safety: Safety::Safe, how: "Close VS Code first. workspaceStorage holds state for old projects.", action: KnownAction::Quarantine },
    KnownLocation { id: "npm_cache", label: "npm cache", paths: &["%LOCALAPPDATA%\\npm-cache"], safety: Safety::Safe, how: "Runs `npm cache clean --force`.", action: KnownAction::Command("npm", &["cache", "clean", "--force"]) },
    KnownLocation { id: "yarn_cache", label: "Yarn cache", paths: &["%LOCALAPPDATA%\\Yarn\\Cache"], safety: Safety::Safe, how: "Runs `yarn cache clean`.", action: KnownAction::Command("yarn", &["cache", "clean"]) },
    KnownLocation { id: "pnpm_store", label: "pnpm store", paths: &["%LOCALAPPDATA%\\pnpm\\store", "%LOCALAPPDATA%\\pnpm-store"], safety: Safety::Safe, how: "Runs `pnpm store prune`.", action: KnownAction::Command("pnpm", &["store", "prune"]) },
    KnownLocation { id: "pip_cache", label: "pip cache", paths: &["%LOCALAPPDATA%\\pip\\Cache"], safety: Safety::Safe, how: "Runs `pip cache purge`.", action: KnownAction::Command("pip", &["cache", "purge"]) },
    KnownLocation { id: "nuget", label: "NuGet packages", paths: &["%USERPROFILE%\\.nuget\\packages"], safety: Safety::Safe, how: "Runs `dotnet nuget locals all --clear`.", action: KnownAction::Command("dotnet", &["nuget", "locals", "all", "--clear"]) },
    KnownLocation { id: "cargo_registry", label: "Cargo download cache", paths: &["%USERPROFILE%\\.cargo\\registry\\cache", "%USERPROFILE%\\.cargo\\git\\db"], safety: Safety::Safe, how: "Downloaded crate archives; Cargo fetches them again when a build needs them.", action: KnownAction::Quarantine },
    KnownLocation { id: "steam_shaders", label: "Steam shader cache", paths: &["%ProgramFiles(x86)%\\Steam\\steamapps\\shadercache"], safety: Safety::Safe, how: "Close Steam first. Rebuilt per game; first launches stutter briefly.", action: KnownAction::Quarantine },
    KnownLocation { id: "jetbrains_caches", label: "JetBrains IDE caches", paths: &["%LOCALAPPDATA%\\JetBrains\\*\\caches", "%LOCALAPPDATA%\\JetBrains\\*\\index"], safety: Safety::Safe, how: "Close the IDE first. Re-indexes projects on next open.", action: KnownAction::Quarantine },
    KnownLocation { id: "gradle_caches", label: "Gradle caches", paths: &["%USERPROFILE%\\.gradle\\caches"], safety: Safety::Safe, how: "Redownloaded on the next Android/Gradle build.", action: KnownAction::Quarantine },
    KnownLocation { id: "unity_cache", label: "Unity global cache", paths: &["%LOCALAPPDATA%\\Unity\\cache", "%LOCALAPPDATA%\\Unity\\Caches"], safety: Safety::Safe, how: "Close Unity first; packages redownload when needed.", action: KnownAction::Quarantine },
    KnownLocation { id: "unity_asset_store", label: "Unity Asset Store downloads", paths: &["%APPDATA%\\Unity\\Asset Store-5.x"], safety: Safety::Review, how: "Downloaded .unitypackage files; redownloadable from Package Manager → My Assets.", action: KnownAction::Quarantine },
    KnownLocation { id: "docker", label: "Docker data", paths: &["%LOCALAPPDATA%\\Docker\\wsl"], safety: Safety::TradeOff, how: "Runs `docker system prune -a` (removes stopped containers, unused images, networks and build cache; volumes are kept). Start Docker Desktop first. The disk file shrinks only after compacting.", action: KnownAction::Command("docker", &["system", "prune", "-a", "-f"]) },
    KnownLocation { id: "wsl_disks", label: "WSL distro disks", paths: &["%LOCALAPPDATA%\\Packages\\*\\LocalState\\ext4.vhdx", "%LOCALAPPDATA%\\wsl\\*\\ext4.vhdx"], safety: Safety::TradeOff, how: "Clean inside the distro, then `wsl --shutdown` and compact with diskpart → select vdisk file=\"<path>\" → compact vdisk.", action: KnownAction::Manual },
    KnownLocation { id: "hiberfil", label: "Hibernation file", paths: &["%SystemDrive%\\hiberfil.sys"], safety: Safety::TradeOff, how: "`powercfg /h off` frees it but disables hibernate and Fast Startup; `powercfg /h /type reduced` shrinks it and keeps Fast Startup. Needs admin.", action: KnownAction::Manual },
    KnownLocation { id: "pagefile", label: "Page file", paths: &["%SystemDrive%\\pagefile.sys"], safety: Safety::Leave, how: "Managed by Windows. Leave it.", action: KnownAction::Manual },
];

#[cfg(not(windows))]
pub const KNOWN_LOCATIONS: &[KnownLocation] = &[
    KnownLocation { id: "user_cache", label: "App caches", paths: &["%HOME%/.cache", "%HOME%/Library/Caches"], safety: Safety::Safe, how: "Apps rebuild these.", action: KnownAction::Quarantine },
    KnownLocation { id: "npm_cache", label: "npm cache", paths: &["%HOME%/.npm"], safety: Safety::Safe, how: "Runs `npm cache clean --force`.", action: KnownAction::Command("npm", &["cache", "clean", "--force"]) },
    KnownLocation { id: "trash", label: "Trash", paths: &["%HOME%/.local/share/Trash", "%HOME%/.Trash"], safety: Safety::Safe, how: "Empty the trash.", action: KnownAction::Manual },
];
