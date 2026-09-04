use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

const RESOURCE_DIR: &str = "resources/binaries";

/// Windows: 不给子进程分配控制台。
///
/// `ffmpeg.exe` / `ffprobe.exe` 是 console 子系统程序。GUI 宿主自己没有控制台
/// （`windows_subsystem = "windows"`），所以系统会为每个子进程新建一个——那就是
/// 播放视频时冒出来的黑窗口。这个标志让子进程完全不带控制台运行，stdout/stderr
/// 管道不受影响。
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 构造一条指向媒体工具的命令。
///
/// 所有 ffmpeg/ffprobe 调用都必须走这里，否则 Windows 上会冒出控制台黑窗口。
pub(crate) fn media_command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub(crate) fn ffmpeg_executable() -> PathBuf {
    resolve("CDITOR_FFMPEG", "ffmpeg")
}

pub(crate) fn ffprobe_executable() -> PathBuf {
    resolve("CDITOR_FFPROBE", "ffprobe")
}

fn resolve(env_var: &str, tool: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(env_var).filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    candidates(tool)
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(tool))
}

fn candidates(tool: &str) -> Vec<PathBuf> {
    let file_name = format!("{tool}-{}{}", target_triple(), executable_extension());
    let plain_name = format!("{tool}{}", executable_extension());
    let mut candidates = Vec::new();
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(Path::new(manifest_dir).join(RESOURCE_DIR).join(&file_name));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(RESOURCE_DIR).join(&file_name));
        candidates.push(dir.join("binaries").join(&file_name));
        candidates.push(dir.join(RESOURCE_DIR).join(&plain_name));
        #[cfg(target_os = "macos")]
        {
            candidates.push(dir.join("../Resources/binaries").join(&file_name));
            candidates.push(dir.join("../Resources").join(RESOURCE_DIR).join(&file_name));
        }
    }
    if let Some(resource_dir) = std::env::var_os("CDITOR_RESOURCE_DIR") {
        candidates.push(PathBuf::from(&resource_dir).join(&file_name));
        candidates.push(PathBuf::from(resource_dir).join(&plain_name));
    }
    candidates
}

const fn executable_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

const fn target_triple() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_name_is_platform_specific() {
        let names = candidates("ffmpeg");
        assert!(
            names
                .iter()
                .any(|path| path.to_string_lossy().contains("ffmpeg-"))
        );
    }
}
