//! A bounded descriptor manifest. Built off UI; verified by the existing probe
//! worker after watches attach and before any decoded Markdown is published.
use files_core::relative_source::{Opened, Proof};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
pub struct Plan {
    pub root: PathBuf,
    pub document: PathBuf,
    pub proof: Proof,
    pub resources: Arc<Vec<(PathBuf, Proof)>>,
    pub directories: Vec<PathBuf>,
    pub required_directories: usize,
    intents: Arc<Vec<PathBuf>>,
    direct: bool,
}
impl Plan {
    #[cfg(test)]
    pub fn new(
        root: PathBuf,
        document: PathBuf,
        proof: Proof,
        resources: Arc<Vec<(PathBuf, Proof)>>,
        direct: bool,
    ) -> io::Result<Self> {
        let intents = Arc::new(resources.iter().map(|(p, _)| p.clone()).collect());
        Self::with_intents(root, document, proof, resources, intents, direct)
    }
    pub fn with_intents(
        root: PathBuf,
        document: PathBuf,
        proof: Proof,
        resources: Arc<Vec<(PathBuf, Proof)>>,
        intents: Arc<Vec<PathBuf>>,
        direct: bool,
    ) -> io::Result<Self> {
        if resources.len() > 8 || intents.len() > 8 {
            return Err(io::Error::other("Demasiados recursos en el manifiesto"));
        }
        // Charge names before join/allocation. No I/O here; caller is supervisor.
        let mut bytes = 0usize;
        for path in std::iter::once(&document)
            .chain(intents.iter())
            .chain(resources.iter().map(|(p, _)| p))
        {
            let len = path.as_os_str().as_encoded_bytes().len();
            if path.as_os_str().as_encoded_bytes().contains(&0)
                || len == 0
                || len > 4096
                || !path
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                return Err(io::Error::other("Manifiesto de rutas no normalizadas"));
            }
            bytes = bytes
                .checked_add(len)
                .ok_or_else(|| io::Error::other("Manifiesto demasiado grande"))?;
            if bytes > 68 * 1024 {
                return Err(io::Error::other("Manifiesto de nombres mayor de68KiB"));
            }
        }
        if !root.is_absolute()
            || root.as_os_str().len() > 4096
            || root.as_os_str().as_encoded_bytes().contains(&0)
            || resources.iter().any(|(_, p)| p.root != proof.root)
        {
            return Err(io::Error::other(
                "Raíz del manifiesto no coincide con sus recursos",
            ));
        }
        let mut plan = Self {
            root,
            document,
            proof,
            resources,
            directories: Vec::new(),
            required_directories: 0,
            intents,
            direct,
        };
        let mut directory_bytes = 0usize;
        let add = |dirs: &mut Vec<PathBuf>, path: &Path, bytes: &mut usize| -> io::Result<()> {
            if dirs.iter().any(|p| p == path) {
                return Ok(());
            }
            let len = path.as_os_str().len();
            let next = bytes
                .checked_add(len)
                .ok_or_else(|| io::Error::other("Watches demasiado grandes"))?;
            if dirs.len() == 32 || len > 4096 || next > 64 * 1024 {
                return Err(io::Error::other(
                    "Documento excede32directorios/64KiB de watches",
                ));
            }
            dirs.push(path.to_path_buf());
            *bytes = next;
            Ok(())
        };
        if let Some(parent) = plan.root.parent() {
            add(&mut plan.directories, parent, &mut directory_bytes)?;
        }
        add(&mut plan.directories, &plan.root, &mut directory_bytes)?;
        for (index, relative) in std::iter::once(&plan.document)
            .chain(plan.intents.iter())
            .enumerate()
        {
            if plan
                .root
                .as_os_str()
                .len()
                .checked_add(relative.as_os_str().len())
                .and_then(|n| n.checked_add(1))
                .is_none_or(|n| n > 4096)
            {
                return Err(io::Error::other("Ruta de recurso mayor de4096B"));
            }
            let joined = plan.root.join(relative);
            let mut parent = joined.parent();
            while let Some(path) = parent {
                if !path.starts_with(&plan.root) {
                    break;
                }
                add(&mut plan.directories, path, &mut directory_bytes)?;
                if path == plan.root {
                    break;
                }
                parent = path.parent();
            }
            if index == 0 {
                plan.required_directories = plan.directories.len();
            }
        }
        Ok(plan)
    }
    /// Change detection for failed/stale manifests; never authorizes publication.
    /// At most9 metadata outcomes. Keeps unavailable→replacement transitions
    /// observable even if inotify lost an event while the document showed error.
    pub fn observe(&self) -> Vec<Result<Proof, Option<i32>>> {
        let mut values = Vec::with_capacity(1 + self.intents.len());
        let document = if self.direct {
            files_core::media_source::open(&self.root.join(&self.document)).and_then(|(_, file)| {
                let root = files_core::relative_source::DirectoryIdentity::from_metadata(
                    &std::fs::metadata(&self.root)?,
                )?;
                Ok(Proof { root, file })
            })
        } else {
            Opened::open(&self.root, &self.document).map(|o| o.proof)
        };
        values.push(document.map_err(|e| e.raw_os_error()));
        for relative in self.intents.iter() {
            values.push(
                Opened::open(&self.root, relative)
                    .map(|o| o.proof)
                    .map_err(|e| e.raw_os_error()),
            );
        }
        values
    }
    pub fn verify(&self) -> io::Result<files_core::media_source::Revision> {
        let direct_path = self.root.join(&self.document);
        let direct_file = if self.direct {
            let (file, revision) = files_core::media_source::open(&direct_path)?;
            if revision != self.proof.file
                || files_core::relative_source::DirectoryIdentity::from_metadata(
                    &std::fs::metadata(&self.root)?,
                )? != self.proof.root
            {
                return Err(io::Error::other("El documento o su raíz cambiaron"));
            }
            Some(file)
        } else {
            None
        };
        let document = if self.direct {
            None
        } else {
            let opened = Opened::open(&self.root, &self.document)?;
            opened.expect(self.proof)?;
            Some(opened)
        };
        for (relative, expected) in self.resources.iter() {
            let resource = Opened::open(&self.root, relative)?;
            resource.expect(*expected)?;
            resource.revalidate()?;
        }
        if let Some(document) = document {
            document.revalidate()?;
        }
        if let Some(file) = direct_file {
            self.proof.file.revalidate(&file, &direct_path)?;
            if files_core::relative_source::DirectoryIdentity::from_metadata(&std::fs::metadata(
                &self.root,
            )?)? != self.proof.root
            {
                return Err(io::Error::other("Raíz de documento cambiada"));
            }
        }
        Ok(self.proof.file)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use files_core::{media_source::Revision, relative_source::DirectoryIdentity};
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "document-source-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            std::fs::create_dir_all(root.join("root/sub/nested")).unwrap();
            std::fs::write(root.join("root/document.md"), b"# document").unwrap();
            std::fs::write(root.join("root/sub/nested/image"), b"red").unwrap();
            Self(root)
        }
        fn plan(&self) -> Plan {
            let root = self.0.join("root");
            let document = Opened::open(&root, Path::new("document.md")).unwrap().proof;
            let image = Opened::open(&root, Path::new("sub/nested/image"))
                .unwrap()
                .proof;
            Plan::new(
                root,
                "document.md".into(),
                document,
                Arc::new(vec![("sub/nested/image".into(), image)]),
                true,
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn replaced_between_decode_and_publication_fails_manifest_verification() {
        let f = Fixture::new();
        let plan = f.plan();
        plan.verify().unwrap();
        let before = plan.observe();
        std::fs::write(f.0.join("new"), b"blu").unwrap();
        std::fs::rename(f.0.join("new"), f.0.join("root/sub/nested/image")).unwrap();
        assert!(plan.verify().is_err());
        assert_ne!(before, plan.observe());
        f.plan().verify().unwrap();
    }
    #[test]
    fn observes_unavailable_then_replacement_and_watches_all_ancestors() {
        let f = Fixture::new();
        let plan = f.plan();
        for path in [
            f.0.clone(),
            f.0.join("root"),
            f.0.join("root/sub"),
            f.0.join("root/sub/nested"),
        ] {
            assert!(plan.directories.contains(&path));
        }
        std::fs::remove_file(f.0.join("root/sub/nested/image")).unwrap();
        let missing = plan.observe();
        assert!(missing[1].is_err());
        std::fs::write(f.0.join("root/sub/nested/image"), b"blu").unwrap();
        assert_ne!(missing, plan.observe());
        assert!(plan.verify().is_err());
    }
    #[test]
    fn direct_document_parent_alias_remains_supported_relative_policy_stays_strict() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let plan = f.plan();
        let alias = f.0.join("alias");
        symlink("root", &alias).unwrap();
        let direct = Plan::new(
            alias.clone(),
            "document.md".into(),
            plan.proof,
            Arc::new(Vec::new()),
            true,
        )
        .unwrap();
        direct.verify().unwrap();
        let relative = Plan::new(
            alias,
            "document.md".into(),
            plan.proof,
            Arc::new(Vec::new()),
            false,
        )
        .unwrap();
        assert!(relative.verify().is_err());
    }
    #[test]
    fn missing_resource_intent_survives_without_rgb_and_detects_recreation() {
        let f = Fixture::new();
        let old = f.plan();
        std::fs::remove_file(f.0.join("root/sub/nested/image")).unwrap();
        let plan = Plan::with_intents(
            old.root.clone(),
            old.document.clone(),
            old.proof,
            Arc::new(Vec::new()),
            Arc::new(vec!["sub/nested/image".into()]),
            true,
        )
        .unwrap();
        plan.verify().unwrap();
        let missing = plan.observe();
        assert!(missing[1].is_err());
        assert!(plan.directories.contains(&f.0.join("root/sub/nested")));
        std::fs::write(f.0.join("root/sub/nested/image"), b"new").unwrap();
        assert_ne!(missing, plan.observe());
        // Missing/budget-rejected resources cannot authorize pixels; only
        // accepted proof-bound resources participate in publication verification.
        assert!(plan.resources.is_empty());
        plan.verify().unwrap();
    }
    #[test]
    fn directory_count_and_manifest_caps_reject_before_watching() {
        let proof = Proof {
            root: DirectoryIdentity {
                device: 1,
                inode: 2,
            },
            file: Revision {
                device: 1,
                inode: 3,
                bytes: 3,
                modified_seconds: 0,
                modified_nanoseconds: 0,
                changed_seconds: 0,
                changed_nanoseconds: 0,
            },
        };
        assert!(
            Plan::new(
                "/fixture".into(),
                "document.md".into(),
                proof,
                Arc::new(vec![("image".into(), proof); 9]),
                true
            )
            .is_err()
        );
        let long = PathBuf::from(format!("{}image", "sub/".repeat(33)));
        assert!(
            Plan::new(
                "/fixture".into(),
                "document.md".into(),
                proof,
                Arc::new(vec![(long, proof)]),
                true
            )
            .is_err()
        );
        assert!(
            Plan::new(
                "/fixture".into(),
                "../escape".into(),
                proof,
                Arc::new(Vec::new()),
                true
            )
            .is_err()
        );
        let mut alien = proof;
        alien.root.inode += 1;
        assert!(
            Plan::new(
                "/fixture".into(),
                "document.md".into(),
                proof,
                Arc::new(vec![("image".into(), alien)]),
                true
            )
            .is_err()
        );
    }
}
