#[tauri::command]
pub fn select_directory() -> Result<Option<String>, String> {
    platform_select_directory()
}

#[tauri::command]
pub fn open_directory(path: String) -> Result<(), String> {
    let directory = std::path::PathBuf::from(path.trim());

    if directory.as_os_str().is_empty() {
        return Err("目录路径为空。".to_string());
    }

    if !directory.exists() {
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("目录不存在且无法创建：{error}"))?;
    }

    if !directory.is_dir() {
        return Err("路径存在但不是有效目录。".to_string());
    }

    platform_open_directory(&directory)
}

#[tauri::command]
pub fn reveal_file(path: String) -> Result<(), String> {
    let target = std::path::PathBuf::from(path.trim());

    if target.as_os_str().is_empty() {
        return Err("文件路径为空。".to_string());
    }

    if !target.exists() {
        return Err("文件不存在，无法定位。".to_string());
    }

    let target = target
        .canonicalize()
        .map_err(|error| format!("无法解析文件路径：{error}"))?;

    platform_reveal_file(&target)
}

#[cfg(target_os = "windows")]
fn platform_open_directory(directory: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("无法打开系统文件管理器：{error}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn platform_reveal_file(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", target.display()))
        .spawn()
        .map_err(|error| format!("无法在资源管理器中定位文件：{error}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_open_directory(directory: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("无法打开系统文件管理器：{error}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_reveal_file(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(target)
        .spawn()
        .map_err(|error| format!("无法在 Finder 中定位文件：{error}"))?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_open_directory(directory: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("无法打开系统文件管理器：{error}"))?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_reveal_file(target: &std::path::Path) -> Result<(), String> {
    let directory = target
        .parent()
        .ok_or_else(|| "无法解析文件所在目录。".to_string())?;
    platform_open_directory(directory)
}

#[cfg(target_os = "windows")]
fn platform_select_directory() -> Result<Option<String>, String> {
    use windows::core::{HRESULT, PWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };

    const HRESULT_FROM_WIN32_CANCELLED: HRESULT = HRESULT(0x800704C7u32 as i32);
    const RPC_E_CHANGED_MODE: HRESULT = HRESULT(0x80010106u32 as i32);

    unsafe {
        let init_result = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninitialize = init_result.is_ok();

        if init_result.is_err() && init_result != RPC_E_CHANGED_MODE {
            return Err(format!(
                "无法初始化系统目录选择器：{}",
                init_result.message()
            ));
        }

        let result = (|| {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|error| format!("无法打开系统目录选择器：{error}"))?;

            let options = dialog
                .GetOptions()
                .map_err(|error| format!("无法读取目录选择器配置：{error}"))?
                | FOS_PICKFOLDERS
                | FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST;
            dialog
                .SetOptions(options)
                .map_err(|error| format!("无法配置目录选择器：{error}"))?;

            match dialog.Show(None) {
                Ok(()) => {}
                Err(error) if error.code() == HRESULT_FROM_WIN32_CANCELLED => return Ok(None),
                Err(error) => return Err(format!("目录选择失败：{error}")),
            }

            let item = dialog
                .GetResult()
                .map_err(|error| format!("无法读取选择结果：{error}"))?;
            let display_name: PWSTR = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|error| format!("无法解析目录路径：{error}"))?;

            let path_result = display_name.to_string();
            CoTaskMemFree(Some(display_name.0 as _));
            let path = path_result.map_err(|error| format!("无法转换目录路径：{error}"))?;

            Ok(Some(path))
        })();

        if should_uninitialize {
            CoUninitialize();
        }

        result
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_select_directory() -> Result<Option<String>, String> {
    Err("当前平台暂未实现系统目录选择器，请手动输入目录路径。".to_string())
}
