//! Profile 存储：目录内每 Profile 一个 JSON 文件，原子写（tmp + rename）。

use anyhow::{Context, Result};
use camoforge_protocol::Profile;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ProfileStore {
    dir: PathBuf,
    profiles: Arc<std::sync::RwLock<BTreeMap<String, Profile>>>,
}

impl ProfileStore {
    pub fn open(base: &Path) -> Result<Self> {
        let dir = base.join("profiles");
        std::fs::create_dir_all(&dir)?;
        let store = Self {
            dir,
            profiles: Arc::new(std::sync::RwLock::new(BTreeMap::new())),
        };
        store.reload()?;
        Ok(store)
    }

    pub fn reload(&self) -> Result<()> {
        let mut map = BTreeMap::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<Profile>(&s).ok())
            {
                Some(p) => {
                    map.insert(p.id.clone(), p);
                }
                None => {
                    // 单个损坏文件不拖垮整体，改名隔离
                    let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                }
            }
        }
        *self.profiles.write().unwrap() = map;
        Ok(())
    }

    pub fn list(&self) -> Vec<Profile> {
        let mut profiles: Vec<Profile> = self.profiles.read().unwrap().values().cloned().collect();
        // 稳定顺序：新创建的在前（created_at 倒序），同名按名称排
        profiles.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        profiles
    }

    pub fn save(&self, profile: &Profile) -> Result<()> {
        let mut p = profile.clone();
        p.updated_at = camoforge_protocol::unix_ts();
        let path = self.path_of(&p.id);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&p).context("serialize profile")?;
        std::fs::write(&tmp, json.as_bytes()).context("write tmp")?;
        std::fs::rename(&tmp, &path).context("atomic rename")?;
        self.profiles
            .write()
            .unwrap()
            .insert(p.id.clone(), p.clone());
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.profiles.write().unwrap().remove(id);
        let path = self.path_of(id);
        if path.exists() {
            // 软删：改名保留（reload 只读 .json，.bak 不影响列表），误删可手动找回
            let bak = path.with_extension("json.bak");
            std::fs::rename(&path, &bak).or_else(|_| std::fs::remove_file(&path))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_delete_roundtrip() {
        let base = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(base.path()).unwrap();
        let mut p = Profile::new("测试身份");
        p.notes = "hello".into();
        store.save(&p).unwrap();

        let reloaded = ProfileStore::open(base.path()).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].name, "测试身份");
        assert_eq!(reloaded.list()[0].notes, "hello");

        assert!(reloaded.delete(&p.id).unwrap());
        assert!(reloaded.list().is_empty());
    }

    #[test]
    fn corrupt_file_isolated() {
        let base = tempfile::tempdir().unwrap();
        let store = ProfileStore::open(base.path()).unwrap();
        let p = Profile::new("ok");
        store.save(&p).unwrap();
        std::fs::write(base.path().join("profiles/bad.json"), "{not json").unwrap();
        let reloaded = ProfileStore::open(base.path()).unwrap();
        assert_eq!(reloaded.list().len(), 1);
    }
}
