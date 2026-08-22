pub use anyhow::{Result, anyhow};
use async_trait::async_trait;
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static PROJECT_ROOT: Lazy<Mutex<Option<Arc<dyn DirectoryEntry>>>> = Lazy::new(|| Mutex::new(None));

pub fn set_project_root(root: Arc<dyn DirectoryEntry>) {
    let mut lock = PROJECT_ROOT.lock().unwrap();
    *lock = Some(root);
}

pub fn get_project_root() -> Option<Arc<dyn DirectoryEntry>> {
    let lock = PROJECT_ROOT.lock().unwrap();
    lock.clone()
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait FileEntry: std::fmt::Debug + Send + Sync {
    fn name(&self) -> String;
    fn path(&self) -> String;
    async fn read(&self) -> Result<Vec<u8>>;
    async fn write(&self, data: &[u8]) -> Result<()>;
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait DirectoryEntry: std::fmt::Debug + Send + Sync {
    fn name(&self) -> String;
    fn path(&self) -> String;
    async fn list_entries(&self) -> Result<Vec<Entry>>;
    async fn get_file(&self, name: &str, create: bool) -> Result<Box<dyn FileEntry>>;
    async fn get_directory(&self, name: &str, create: bool) -> Result<Box<dyn DirectoryEntry>>;
    async fn delete_entry(&self, name: &str) -> Result<()>;
    fn as_any(&self) -> &dyn std::any::Any;
}

pub enum Entry {
    File(Box<dyn FileEntry>),
    Directory(Box<dyn DirectoryEntry>),
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Entry::File(file) => write!(f, "File({:?})", file),
            Entry::Directory(dir) => write!(f, "Directory({:?})", dir),
        }
    }
}

// Native implementations
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct NativeFile {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl FileEntry for NativeFile {
    fn name(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    }
    fn path(&self) -> String {
        to_file_uri(&self.path.to_string_lossy())
    }
    async fn read(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.path).map_err(|e| anyhow!("Read failed: {}", e))
    }
    async fn write(&self, data: &[u8]) -> Result<()> {
        std::fs::write(&self.path, data).map_err(|e| anyhow!("Write failed: {}", e))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct NativeDirectory {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl DirectoryEntry for NativeDirectory {
    fn name(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    }
    fn path(&self) -> String {
        to_file_uri(&self.path.to_string_lossy())
    }
    async fn list_entries(&self) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&self.path).map_err(|e| anyhow!("ReadDir failed: {}", e))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                entries.push(Entry::File(Box::new(NativeFile { path })));
            } else if path.is_dir() {
                entries.push(Entry::Directory(Box::new(NativeDirectory { path })));
            }
        }
        Ok(entries)
    }
    async fn get_file(&self, name: &str, create: bool) -> Result<Box<dyn FileEntry>> {
        let path = self.path.join(name);
        if create && !path.exists() {
            std::fs::File::create(&path).map_err(|e| anyhow!("Create failed: {}", e))?;
        }
        Ok(Box::new(NativeFile { path }))
    }
    async fn get_directory(&self, name: &str, create: bool) -> Result<Box<dyn DirectoryEntry>> {
        let path = self.path.join(name);
        if create && !path.exists() {
            std::fs::create_dir_all(&path).map_err(|e| anyhow!("CreateDir failed: {}", e))?;
        }
        Ok(Box::new(NativeDirectory { path }))
    }
    async fn delete_entry(&self, name: &str) -> Result<()> {
        let path = self.path.join(name);
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| anyhow!("Delete file failed: {}", e))
        } else {
            std::fs::remove_dir_all(path).map_err(|e| anyhow!("Delete dir failed: {}", e))
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Web implementations
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct SendFileHandle(pub web_sys::FileSystemFileHandle);
#[cfg(target_arch = "wasm32")]
unsafe impl Send for SendFileHandle {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for SendFileHandle {}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct SendDirectoryHandle(pub web_sys::FileSystemDirectoryHandle);
#[cfg(target_arch = "wasm32")]
unsafe impl Send for SendDirectoryHandle {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for SendDirectoryHandle {}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct WebFile {
    handle: SendFileHandle,
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl FileEntry for WebFile {
    fn name(&self) -> String {
        self.handle.0.name()
    }
    fn path(&self) -> String {
        self.handle.0.name()
    }
    async fn read(&self) -> Result<Vec<u8>> {
        use wasm_bindgen_futures::JsFuture;
        let file_promise = self.handle.0.get_file();
        let file_value = JsFuture::from(file_promise)
            .await
            .map_err(|e| anyhow!("GetFile failed: {:?}", e))?;
        let file = web_sys::File::from(file_value);
        let array_buffer_promise = file.array_buffer();
        let array_buffer_value = JsFuture::from(array_buffer_promise)
            .await
            .map_err(|e| anyhow!("ArrayBuffer failed: {:?}", e))?;
        let array_buffer = js_sys::ArrayBuffer::from(array_buffer_value);
        Ok(js_sys::Uint8Array::new(&array_buffer).to_vec())
    }
    async fn write(&self, data: &[u8]) -> Result<()> {
        use wasm_bindgen_futures::JsFuture;
        let writable_promise = self.handle.0.create_writable();
        let writable_value = JsFuture::from(writable_promise)
            .await
            .map_err(|e| anyhow!("CreateWritable failed: {:?}", e))?;
        let writable = web_sys::FileSystemWritableFileStream::from(writable_value);

        let uint8_array = js_sys::Uint8Array::from(data);
        JsFuture::from(
            writable
                .write_with_buffer_source(&uint8_array)
                .map_err(|e| anyhow!("Write failed: {:?}", e))?,
        )
        .await
        .map_err(|e| anyhow!("Write await failed: {:?}", e))?;
        JsFuture::from(writable.close())
            .await
            .map_err(|e| anyhow!("Close failed: {:?}", e))?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct WebDirectory {
    pub handle: SendDirectoryHandle,
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl DirectoryEntry for WebDirectory {
    fn name(&self) -> String {
        self.handle.0.name()
    }
    fn path(&self) -> String {
        self.handle.0.name()
    }
    async fn list_entries(&self) -> Result<Vec<Entry>> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let mut entries = Vec::new();
        let iterator = self.handle.0.values();
        loop {
            let next_promise = iterator
                .next()
                .map_err(|e| anyhow!("Next failed: {:?}", e))?;
            let next_value = JsFuture::from(next_promise)
                .await
                .map_err(|e| anyhow!("Next await failed: {:?}", e))?;
            let next_obj = js_sys::Object::from(next_value);
            let done = js_sys::Reflect::get(&next_obj, &"done".into())
                .unwrap()
                .as_bool()
                .unwrap_or(true);
            if done {
                break;
            }

            let value = js_sys::Reflect::get(&next_obj, &"value".into()).unwrap();
            let handle = web_sys::FileSystemHandle::from(value);

            let kind = js_sys::Reflect::get(&handle, &"kind".into())
                .unwrap()
                .as_string()
                .unwrap_or_default();

            if kind == "file" {
                entries.push(Entry::File(Box::new(WebFile {
                    handle: SendFileHandle(handle.dyn_into().unwrap()),
                })));
            } else {
                entries.push(Entry::Directory(Box::new(WebDirectory {
                    handle: SendDirectoryHandle(handle.dyn_into().unwrap()),
                })));
            }
        }
        Ok(entries)
    }
    async fn get_file(&self, name: &str, create: bool) -> Result<Box<dyn FileEntry>> {
        use wasm_bindgen_futures::JsFuture;
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"create".into(), &create.into()).ok();

        let promise = extGetFileHandle(&self.handle.0, name, &options.unchecked_into())
            .map_err(|e| anyhow!("extGetFileHandle failed: {:?}", e))?;
        let value = JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("GetFileHandle failed: {:?}", e))?;
        Ok(Box::new(WebFile {
            handle: SendFileHandle(value.into()),
        }))
    }
    async fn get_directory(&self, name: &str, create: bool) -> Result<Box<dyn DirectoryEntry>> {
        use wasm_bindgen_futures::JsFuture;
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"create".into(), &create.into()).ok();

        let promise = extGetDirectoryHandle(&self.handle.0, name, &options.unchecked_into())
            .map_err(|e| anyhow!("extGetDirectoryHandle failed: {:?}", e))?;
        let value = JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("GetDirectoryHandle failed: {:?}", e))?;
        Ok(Box::new(WebDirectory {
            handle: SendDirectoryHandle(value.into()),
        }))
    }
    async fn delete_entry(&self, name: &str) -> Result<()> {
        use wasm_bindgen_futures::JsFuture;
        let promise = self.handle.0.remove_entry(name);
        JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("RemoveEntry failed: {:?}", e))?;
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = showDirectoryPicker)]
    fn show_directory_picker() -> js_sys::Promise;

    #[wasm_bindgen(js_name = showOpenFilePicker)]
    fn show_open_file_picker(options: &js_sys::Object) -> js_sys::Promise;

}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export function extGetFileHandle(dirHandle, name, options) {
    return dirHandle.getFileHandle(name, options);
}
export function extGetDirectoryHandle(dirHandle, name, options) {
    return dirHandle.getDirectoryHandle(name, options);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    fn extGetFileHandle(
        dirHandle: &web_sys::FileSystemDirectoryHandle,
        name: &str,
        options: &js_sys::Object,
    ) -> Result<js_sys::Promise, JsValue>;

    #[wasm_bindgen(catch)]
    fn extGetDirectoryHandle(
        dirHandle: &web_sys::FileSystemDirectoryHandle,
        name: &str,
        options: &js_sys::Object,
    ) -> Result<js_sys::Promise, JsValue>;
}

pub async fn pick_file_entry(
    filter_name: String,
    extensions: Vec<String>,
) -> Result<Option<Box<dyn FileEntry>>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let extensions_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter(&filter_name, &extensions_refs)
            .pick_file()
            .await;

        if let Some(file_handle) = dialog {
            let path = file_handle.path().to_path_buf();
            Ok(Some(Box::new(NativeFile { path })))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let options = js_sys::Object::new();

        if !extensions.is_empty() {
            let types = js_sys::Array::new();
            let type_obj = js_sys::Object::new();
            js_sys::Reflect::set(&type_obj, &"description".into(), &filter_name.into()).ok();

            let accept = js_sys::Object::new();
            let exts = js_sys::Array::new();
            for ext in extensions {
                let ext_with_dot = if ext.starts_with('.') {
                    ext
                } else {
                    format!(".{}", ext)
                };
                exts.push(&ext_with_dot.into());
            }
            js_sys::Reflect::set(&accept, &"*/*".into(), &exts).ok();
            js_sys::Reflect::set(&type_obj, &"accept".into(), &accept).ok();
            types.push(&type_obj);
            js_sys::Reflect::set(&options, &"types".into(), &types).ok();
        }

        js_sys::Reflect::set(&options, &"multiple".into(), &false.into()).ok();

        let promise = show_open_file_picker(&options);
        let result = JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("showOpenFilePicker failed: {:?}", e))?;
        let handles = result
            .dyn_into::<js_sys::Array>()
            .map_err(|_| anyhow!("Expected array of handles"))?;

        if handles.length() > 0 {
            let handle = handles.get(0);
            let handle: web_sys::FileSystemFileHandle = handle
                .dyn_into()
                .map_err(|_| anyhow!("Expected FileSystemFileHandle"))?;
            Ok(Some(Box::new(WebFile {
                handle: SendFileHandle(handle),
            })))
        } else {
            Ok(None)
        }
    }
}

pub async fn pick_folder_entry() -> Result<Option<Box<dyn DirectoryEntry>>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dialog = rfd::AsyncFileDialog::new().pick_folder().await;

        if let Some(folder_handle) = dialog {
            let path = folder_handle.path().to_path_buf();
            Ok(Some(Box::new(NativeDirectory { path })))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let promise = show_directory_picker();
        let handle_value = JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("showDirectoryPicker failed: {:?}", e))?;
        let handle: web_sys::FileSystemDirectoryHandle = handle_value
            .dyn_into()
            .map_err(|_| anyhow!("Failed to cast to FileSystemDirectoryHandle"))?;
        Ok(Some(Box::new(WebDirectory {
            handle: SendDirectoryHandle(handle),
        })))
    }
}

pub async fn pick_file(filter_name: String, extensions: Vec<String>) -> Result<Option<String>> {
    let entry = pick_file_entry(filter_name, extensions).await?;
    Ok(entry.map(|e| e.path()))
}

pub async fn pick_folder() -> Result<Option<String>> {
    let entry = pick_folder_entry().await?;
    Ok(entry.map(|e| e.path()))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn persist_project_root(root: Arc<dyn DirectoryEntry>) -> Result<()> {
    use directories::ProjectDirs;
    if let Some(proj_dirs) = ProjectDirs::from("com", "adventure-simulator-group", "fabelgeist") {
        let config_dir = proj_dirs.config_dir();
        std::fs::create_dir_all(config_dir)
            .map_err(|e| anyhow!("Failed to create config dir: {}", e))?;
        let path_file = config_dir.join("workspace.txt");
        let path_str = root.path();
        let path_str = path_str.strip_prefix("file:///").unwrap_or(&path_str);
        let path_str = path_str.strip_prefix("file://").unwrap_or(path_str);
        std::fs::write(path_file, path_str)
            .map_err(|e| anyhow!("Failed to write workspace.txt: {}", e))?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn restore_project_root() -> Result<Option<Arc<dyn DirectoryEntry>>> {
    use directories::ProjectDirs;
    if let Some(proj_dirs) = ProjectDirs::from("com", "adventure-simulator-group", "fabelgeist") {
        let path_file = proj_dirs.config_dir().join("workspace.txt");
        if path_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&path_file) {
                let path = std::path::PathBuf::from(content.trim());
                if path.exists() && path.is_dir() {
                    return Ok(Some(Arc::new(NativeDirectory { path })));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export async function idbSaveHandle(handle) {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open("fabelgeist_fs", 1);
        req.onupgradeneeded = (e) => {
            e.target.result.createObjectStore("handles");
        };
        req.onsuccess = (e) => {
            const db = e.target.result;
            const tx = db.transaction("handles", "readwrite");
            const store = tx.objectStore("handles");
            store.put(handle, "project_root");
            tx.oncomplete = () => resolve();
            tx.onerror = () => reject(tx.error);
        };
        req.onerror = () => reject(req.error);
    });
}
export async function idbLoadHandle() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open("fabelgeist_fs", 1);
        req.onupgradeneeded = (e) => {
            e.target.result.createObjectStore("handles");
        };
        req.onsuccess = (e) => {
            const db = e.target.result;
            if (!db.objectStoreNames.contains("handles")) {
                resolve(null);
                return;
            }
            const tx = db.transaction("handles", "readonly");
            const store = tx.objectStore("handles");
            const getReq = store.get("project_root");
            getReq.onsuccess = () => resolve(getReq.result || null);
            getReq.onerror = () => reject(getReq.error);
        };
        req.onerror = () => reject(req.error);
    });
}
export async function idbCheckPermission(handle) {
    const opts = { mode: "read" };
    const status = await handle.queryPermission(opts);
    if (status !== "granted") {
        return await handle.requestPermission(opts);
    }
    return status;
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn idbSaveHandle(handle: &web_sys::FileSystemDirectoryHandle) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    async fn idbLoadHandle() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn idbCheckPermission(
        handle: &web_sys::FileSystemDirectoryHandle,
    ) -> Result<JsValue, JsValue>;
}

#[cfg(target_arch = "wasm32")]
pub async fn persist_project_root(root: Arc<dyn DirectoryEntry>) -> Result<()> {
    if let Some(web_dir) = root.as_any().downcast_ref::<WebDirectory>() {
        idbSaveHandle(&web_dir.handle.0)
            .await
            .map_err(|e| anyhow!("Failed to save handle to IDB: {:?}", e))?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn restore_project_root() -> Result<Option<Arc<dyn DirectoryEntry>>> {
    use wasm_bindgen::JsCast;
    let val = idbLoadHandle()
        .await
        .map_err(|e| anyhow!("Failed to load handle from IDB: {:?}", e))?;
    if val.is_null() || val.is_undefined() {
        return Ok(None);
    }
    let handle: web_sys::FileSystemDirectoryHandle = val
        .dyn_into()
        .map_err(|_| anyhow!("Failed to cast handle"))?;

    // We must request permission to use it again
    let status_val = idbCheckPermission(&handle)
        .await
        .map_err(|e| anyhow!("Check permission failed: {:?}", e))?;
    let status = status_val.as_string().unwrap_or_default();

    if status != "granted" {
        return Ok(None); // Permission denied
    }

    Ok(Some(Arc::new(WebDirectory {
        handle: SendDirectoryHandle(handle),
    })))
}

#[cfg(not(target_arch = "wasm32"))]
fn to_file_uri(path: &str) -> String {
    let path = path.replace("\\", "/");
    if path.starts_with("/") {
        format!("file://{}", path)
    } else {
        format!("file:///{}", path)
    }
}

pub async fn to_data_uri(uri: &str) -> Result<String> {
    if uri.starts_with("data:") {
        return Ok(uri.to_string());
    }

    let bytes = read_bytes(uri).await?;
    let mime = detect_mime(uri);

    use base64::{Engine as _, engine::general_purpose};
    let b64 = general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

pub fn detect_mime(uri: &str) -> &'static str {
    let lower = uri.to_lowercase();
    if lower.ends_with(".glb") {
        "application/octet-stream"
    } else if lower.ends_with(".gltf") {
        "application/json"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

pub async fn read_bytes(uri: &str) -> Result<Vec<u8>> {
    if uri.starts_with("data:") {
        return load_data_uri(uri);
    }
    if uri.starts_with("http") || uri.starts_with("blob:") {
        return load_http(uri).await;
    }

    let mut is_project_relative = false;
    let clean_uri = if uri.starts_with("prism://project/") {
        is_project_relative = true;
        &uri[16..]
    } else if uri.starts_with("file://") {
        if uri.starts_with("file:///") {
            &uri[8..]
        } else {
            &uri[7..]
        }
    } else {
        uri
    };
    let clean_uri = percent_encoding::percent_decode_str(clean_uri).decode_utf8_lossy();

    if let Some(root) = get_project_root() {
        let root_path = root.path();
        let resolve_path = if is_project_relative {
            Some(clean_uri.to_string())
        } else if clean_uri.starts_with(&root_path) {
            let relative = &clean_uri[root_path.len()..];
            Some(
                relative
                    .trim_start_matches(|c| c == '/' || c == '\\')
                    .to_string(),
            )
        } else {
            None
        };

        if let Some(relative_path) = resolve_path {
            if !relative_path.is_empty() {
                let mut current = root.clone();
                let parts: Vec<&str> = relative_path.split(|c| c == '/' || c == '\\').collect();

                if parts.len() > 1 {
                    for i in 0..parts.len() - 1 {
                        if parts[i].is_empty() {
                            continue;
                        }
                        current = Arc::from(current.get_directory(parts[i], false).await?);
                    }
                }

                if let Some(last) = parts.last() {
                    if !last.is_empty() {
                        let file_entry = current.get_file(last, false).await?;
                        return file_entry.read().await;
                    }
                }
            } else {
                return Err(anyhow!("Cannot read a directory as bytes: {}", uri));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        load_file(uri).await
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(anyhow!(
            "File system access failed: URI {} not found in project root and file:// is not supported on web",
            uri
        ))
    }
}

pub async fn write_bytes(uri: &str, data: &[u8]) -> Result<()> {
    if uri.starts_with("data:") || uri.starts_with("http") || uri.starts_with("blob:") {
        return Err(anyhow!("Cannot write to read-only URI: {}", uri));
    }

    let mut is_project_relative = false;
    let clean_uri = if uri.starts_with("prism://project/") {
        is_project_relative = true;
        &uri[16..]
    } else if uri.starts_with("file://") {
        if uri.starts_with("file:///") {
            &uri[8..]
        } else {
            &uri[7..]
        }
    } else {
        uri
    };
    let clean_uri = percent_encoding::percent_decode_str(clean_uri).decode_utf8_lossy();

    if let Some(root) = get_project_root() {
        let root_path = root.path();
        let resolve_path = if is_project_relative {
            Some(clean_uri.to_string())
        } else if clean_uri.starts_with(&root_path) {
            let relative = &clean_uri[root_path.len()..];
            Some(
                relative
                    .trim_start_matches(|c| c == '/' || c == '\\')
                    .to_string(),
            )
        } else {
            None
        };

        if let Some(relative_path) = resolve_path {
            if !relative_path.is_empty() {
                let mut current = root.clone();
                let parts: Vec<&str> = relative_path.split(|c| c == '/' || c == '\\').collect();

                if parts.len() > 1 {
                    for i in 0..parts.len() - 1 {
                        if parts[i].is_empty() {
                            continue;
                        }
                        current = Arc::from(current.get_directory(parts[i], true).await?);
                    }
                }

                if let Some(last) = parts.last() {
                    if !last.is_empty() {
                        let file_entry = current.get_file(last, true).await?;
                        return file_entry.write(data).await;
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = if uri.starts_with("prism://project/") {
            &uri[16..]
        } else {
            uri.strip_prefix("file://").unwrap_or(uri)
        };
        let path = if path.starts_with("/") && path.len() > 3 && path.as_bytes()[2] == b':' {
            &path[1..]
        } else {
            path
        };
        std::fs::write(path, data).map_err(|e| anyhow!("Failed to write file {}: {}", path, e))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(anyhow!(
            "File system write failed: URI {} not found in project root and file:// is not supported on web",
            uri
        ))
    }
}

async fn load_http(uri: &str) -> Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let response = reqwest::get(uri).await?;
        Ok(response.bytes().await?.to_vec())
    }
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        use std::sync::{Mutex, OnceLock};
        static FAILED_FETCHES: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();

        let (tx, rx) = futures_channel::oneshot::channel::<Result<Vec<u8>>>();
        let uri = uri.to_string();

        if let Some(failed) = FAILED_FETCHES.get() {
            if let Ok(guard) = failed.lock() {
                if guard.contains(&uri) {
                    return Err(anyhow!("Fetch failed (cached)"));
                }
            }
        }

        wasm_bindgen_futures::spawn_local(async move {
            let opts = RequestInit::new();
            opts.set_method("GET");
            opts.set_mode(RequestMode::Cors);

            let result: Result<Vec<u8>> = async {
                let request = Request::new_with_str_and_init(&uri, &opts)
                    .map_err(|e| anyhow!("Failed to create request: {:?}", e))?;

                let window = web_sys::window().ok_or_else(|| anyhow!("No window found"))?;
                let resp_value = JsFuture::from(window.fetch_with_request(&request))
                    .await
                    .map_err(|e| anyhow!("Fetch failed: {:?}", e))?;

                let resp: Response = resp_value
                    .dyn_into()
                    .map_err(|e| anyhow!("Failed to cast response: {:?}", e))?;

                if !resp.ok() {
                    return Err(anyhow!("HTTP error: {}", resp.status()));
                }

                let array_buffer_value = JsFuture::from(
                    resp.array_buffer()
                        .map_err(|e| anyhow!("Failed to get array buffer: {:?}", e))?,
                )
                .await
                .map_err(|e| anyhow!("Failed to await array buffer: {:?}", e))?;

                let array_buffer = js_sys::ArrayBuffer::from(array_buffer_value);
                let uint8_array = js_sys::Uint8Array::new(&array_buffer);
                Ok(uint8_array.to_vec())
            }
            .await;

            if result.is_err() && uri.starts_with("blob:") {
                let cache =
                    FAILED_FETCHES.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
                if let Ok(mut guard) = cache.lock() {
                    guard.insert(uri.clone());
                }
            }
            let _ = tx.send(result);
        });

        rx.await.map_err(|_| anyhow!("Channel closed"))?
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn load_file(_uri: &str) -> Result<Vec<u8>> {
    let path = if _uri.starts_with("prism://project/") {
        &_uri[16..]
    } else {
        _uri.strip_prefix("file://").unwrap_or(_uri)
    };
    // Handle Windows paths like file:///C:/path
    let path = if path.starts_with("/") && path.len() > 3 && path.as_bytes()[2] == b':' {
        &path[1..]
    } else {
        path
    };
    std::fs::read(path).map_err(|e| anyhow!("Failed to read file {}: {}", path, e))
}

fn load_data_uri(uri: &str) -> Result<Vec<u8>> {
    let comma_pos = uri.find(',').ok_or_else(|| anyhow!("Invalid data URI"))?;
    let data = &uri[comma_pos + 1..];
    if uri[..comma_pos].contains(";base64") {
        use base64::{Engine as _, engine::general_purpose};
        general_purpose::STANDARD
            .decode(data)
            .map_err(|e| anyhow!("Base64 decode failed: {}", e))
    } else {
        Ok(percent_encoding::percent_decode_str(data).collect())
    }
}

/// Returns the standard sandboxed persistent directory for the application.
/// - Web: Origin Private File System (OPFS) root sandbox directory.
/// - Desktop/Mobile: OS-specific app data/files directory.
pub async fn get_app_directory(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> Result<Arc<dyn DirectoryEntry>> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (qualifier, organization, application);
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;
        let window = web_sys::window().ok_or_else(|| anyhow!("No window found"))?;
        let navigator = window.navigator();
        let storage = navigator.storage();
        let promise = storage.get_directory();
        let handle_value = JsFuture::from(promise)
            .await
            .map_err(|e| anyhow!("getDirectory failed: {:?}", e))?;
        let handle: web_sys::FileSystemDirectoryHandle = handle_value
            .dyn_into()
            .map_err(|_| anyhow!("Failed to cast handle"))?;
        Ok(Arc::new(WebDirectory {
            handle: SendDirectoryHandle(handle),
        }))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut path = None;

        #[cfg(target_os = "android")]
        {
            let pkg_id = format!(
                "{}.{}",
                organization.to_lowercase(),
                application.to_lowercase()
            );
            let android_path = std::path::PathBuf::from(format!("/data/data/{}/files", pkg_id));
            if android_path.exists() {
                path = Some(android_path);
            }
        }

        if path.is_none() {
            if let Some(proj_dirs) =
                directories::ProjectDirs::from(qualifier, organization, application)
            {
                path = Some(proj_dirs.data_dir().to_path_buf());
            }
        }

        let final_path = path.unwrap_or_else(|| std::path::PathBuf::from("./data"));

        std::fs::create_dir_all(&final_path)
            .map_err(|e| anyhow!("Failed to create app directory: {}", e))?;
        Ok(Arc::new(NativeDirectory { path: final_path }))
    }
}
