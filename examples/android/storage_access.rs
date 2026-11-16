use android_activity::AndroidApp;
use jni::{objects::{JObject, JValue}, JavaVM};

const MANAGE_STORAGE_ACTION: &str = "android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION";
const READ_PERMISSION: &str = "android.permission.READ_EXTERNAL_STORAGE";
const WRITE_PERMISSION: &str = "android.permission.WRITE_EXTERNAL_STORAGE";

pub struct StorageAccess<'a>(&'a AndroidApp);

impl<'a> StorageAccess<'a> {
    pub fn new(app: &'a AndroidApp) -> Self { Self(app) }

    fn with_activity<T>(&self, f: impl FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<T>) -> Result<T, String> {
        let vm = unsafe { JavaVM::from_raw(self.0.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(self.0.activity_as_ptr().cast()) };
        let result = f(&mut env, &activity).map_err(|e| e.to_string());
        let _ = activity.into_raw();
        result
    }

    pub fn has_external_storage_access(&self) -> Result<bool, String> {
        self.with_activity(|env, activity| {
            if AndroidApp::sdk_version() < 30 {
                let permission = env.new_string(READ_PERMISSION)?;
                return Ok(env.call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&permission)])?.i()? == 0);
            }
            let environment = env.find_class("android/os/Environment")?;
            Ok(env.call_static_method(environment, "isExternalStorageManager", "()Z", &[])?.z()?)
        })
    }

    pub fn request_external_storage_access(&self) -> Result<(), String> {
        self.with_activity(|env, activity| {
            if AndroidApp::sdk_version() < 30 {
                let permissions = env.new_object_array(2, "java/lang/String", JObject::null())?;
                let read = env.new_string(READ_PERMISSION)?;
                let write = env.new_string(WRITE_PERMISSION)?;
                env.set_object_array_element(&permissions, 0, &read)?;
                env.set_object_array_element(&permissions, 1, &write)?;
                env.call_method(activity, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&permissions), JValue::Int(7301)])?;
                return Ok(());
            }
            let package = env.call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
            let package_name = env.get_string((&package).into())?.to_string_lossy().into_owned();
            let uri_text = env.new_string(format!("package:{package_name}"))?;
            let uri_class = env.find_class("android/net/Uri")?;
            let uri = env.call_static_method(uri_class, "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri_text)])?.l()?;
            let intent_class = env.find_class("android/content/Intent")?;
            let action = env.new_string(MANAGE_STORAGE_ACTION)?;
            let intent = env.new_object(intent_class, "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
            env.call_method(&intent, "setData", "(Landroid/net/Uri;)Landroid/content/Intent;", &[JValue::Object(&uri)])?;
            env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
            Ok(())
        })
    }
}
