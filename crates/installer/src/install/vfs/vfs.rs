/*
Key Design Decisions:
1. Tree Structure with Shared Ownership
Uses Rc<RefCell<VirtualFileNode>> for shared ownership and interior mutability
Parent references use Weak<RefCell<VirtualFileNode>> to avoid reference cycles
Children stored in HashMap for fast lookups

2. Archive vs Native Files
source_archive field indicates if file comes from an archive
archive_path tracks the path within the archive
Root nodes represent physical archive files on disk

3. Efficient Lookups
VfsContext provides quick lookups by HashRelativePath
path_index maps (archive_hash, internal_path) to nodes
archive_locations maps archive hashes to disk paths

4. Type Safety
Uses Option<Hash> to distinguish files (Some) from directories (None)
HashRelativePath struct ensures type safety for archive path references

5. Memory Efficiency
Lazy loading: only create nodes for known paths from the modlist
Shared string storage through PathBuf
Weak parent references prevent memory leaks
This design captures the essential functionality of Wabbajack's VFS while being idiomatic Rust with proper memory management and type safety.
*/


use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::cell::RefCell;

// Hash type - you'd define this based on your hashing implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 8]); // xxHash64 is 8 bytes

#[derive(Debug, Clone)]
pub struct VirtualFileNode {
    /// Name of this file/directory (just the filename, not full path)
    pub name: PathBuf,

    /// Content hash of the file (None for directories)
    pub hash: Option<Hash>,

    /// Size in bytes (0 for directories)
    pub size: u64,

    /// Weak reference to parent node (None for root nodes)
    pub parent: Option<Weak<RefCell<VirtualFileNode>>>,

    /// Child nodes (empty for files, contains subdirs/files for directories)
    pub children: HashMap<PathBuf, Rc<RefCell<VirtualFileNode>>>,

    /// Which physical archive file this virtual file comes from (for archive contents)
    /// None for files that exist directly on disk
    pub source_archive: Option<SourceArchive>,

    /// Path within the source archive (for files inside archives)
    /// Empty for files that exist directly on disk
    pub archive_path: Vec<PathBuf>,

    /// Timestamps (optional)
    pub last_modified: Option<u64>,
    pub last_analyzed: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SourceArchive {
    /// Hash of the archive file itself
    pub archive_hash: Hash,
    /// Path to the archive file on disk
    pub archive_path: PathBuf,
}

impl VirtualFileNode {
    /// Create a new root node (represents a physical archive file)
    pub fn new_archive_root(name: PathBuf, hash: Hash, size: u64, archive_path: PathBuf) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(VirtualFileNode {
            name,
            hash: Some(hash),
            size,
            parent: None,
            children: HashMap::new(),
            source_archive: Some(SourceArchive {
                archive_hash: hash,
                archive_path,
            }),
            archive_path: Vec::new(),
            last_modified: None,
            last_analyzed: None,
        }))
    }

    /// Create a new file node within an archive
    pub fn new_archive_file(
        name: PathBuf,
        hash: Option<Hash>,
        size: u64,
        parent: Rc<RefCell<VirtualFileNode>>,
        archive_path: Vec<PathBuf>,
    ) -> Rc<RefCell<Self>> {
        let source_archive = parent.borrow().source_archive.clone();

        Rc::new(RefCell::new(VirtualFileNode {
            name,
            hash,
            size,
            parent: Some(Rc::downgrade(&parent)),
            children: HashMap::new(),
            source_archive,
            archive_path,
            last_modified: None,
            last_analyzed: None,
        }))
    }

    /// Add a child node
    pub fn add_child(&mut self, child: Rc<RefCell<VirtualFileNode>>) {
        let child_name = child.borrow().name.clone();
        self.children.insert(child_name, child);
    }

    /// Check if this is a directory (has children)
    pub fn is_directory(&self) -> bool {
        !self.children.is_empty()
    }

    /// Check if this is a root node (no parent)
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Check if this file comes from an archive
    pub fn is_from_archive(&self) -> bool {
        self.source_archive.is_some()
    }

    /// Get the full path from root to this node
    pub fn full_path(&self) -> PathBuf {
        let mut path_parts = Vec::new();
        let mut current = Some(Rc::new(RefCell::new(self.clone())));

        while let Some(node_rc) = current {
            let node = node_rc.borrow();
            path_parts.push(node.name.clone());

            current = node.parent.as_ref()
                .and_then(|weak| weak.upgrade());
        }

        path_parts.reverse();
        path_parts.into_iter().collect()
    }

    /// Find a child node by path
    pub fn find_child(&self, path: &Path) -> Option<Rc<RefCell<VirtualFileNode>>> {
        let mut components = path.components();
        let first = components.next()?.as_os_str();

        if let Some(child) = self.children.get(Path::new(first)) {
            if let Some(remaining) = components.as_path().to_str() {
                if remaining.is_empty() {
                    Some(child.clone())
                } else {
                    child.borrow().find_child(Path::new(remaining))
                }
            } else {
                Some(child.clone())
            }
        } else {
            None
        }
    }
}

// HashRelativePath equivalent for Rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HashRelativePath {
    pub hash: Hash,
    pub parts: Vec<PathBuf>,
}

impl HashRelativePath {
    pub fn new(hash: Hash, parts: Vec<PathBuf>) -> Self {
        Self { hash, parts }
    }

    pub fn from_string(hash: Hash, path: &str) -> Self {
        let parts = Path::new(path)
            .components()
            .map(|c| PathBuf::from(c.as_os_str()))
            .collect();
        Self { hash, parts }
    }
}

// VFS Context for managing the virtual file system
pub struct VfsContext {
    /// Root nodes (usually archive files)
    pub roots: HashMap<Hash, Rc<RefCell<VirtualFileNode>>>,

    /// Quick lookup: (archive_hash, internal_path) -> node
    pub path_index: HashMap<HashRelativePath, Rc<RefCell<VirtualFileNode>>>,

    /// Archive hash -> file path on disk
    pub archive_locations: HashMap<Hash, PathBuf>,
}

impl VfsContext {
    pub fn new() -> Self {
        Self {
            roots: HashMap::new(),
            path_index: HashMap::new(),
            archive_locations: HashMap::new(),
        }
    }

    /// Add known archive and its internal structure
    pub fn add_known_archive(&mut self, archive_hash: Hash, archive_path: PathBuf, internal_paths: Vec<HashRelativePath>) {
        // Create root node for the archive
        let root = VirtualFileNode::new_archive_root(
            archive_path.file_name().unwrap().into(),
            archive_hash,
            std::fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0),
            archive_path.clone()
        );

        self.roots.insert(archive_hash, root.clone());
        self.archive_locations.insert(archive_hash, archive_path);

        // Build the internal directory structure
        for hash_path in internal_paths {
            if hash_path.hash == archive_hash {
                self.build_path_in_archive(root.clone(), &hash_path.parts);
            }
        }
    }

    /// Build nested directory structure within an archive
    fn build_path_in_archive(&mut self, root: Rc<RefCell<VirtualFileNode>>, path_parts: &[PathBuf]) {
        let mut current = root;
        let mut archive_path = Vec::new();

        for part in path_parts {
            archive_path.push(part.clone());

            let next = {
                let current_borrowed = current.borrow();
                current_borrowed.children.get(part).cloned()
            };

            if let Some(existing) = next {
                current = existing;
            } else {
                // Create new node
                let new_node = VirtualFileNode::new_archive_file(
                    part.clone(),
                    None, // Hash would be computed during actual indexing
                    0,    // Size would be computed during actual indexing
                    current.clone(),
                    archive_path.clone(),
                );

                current.borrow_mut().add_child(new_node.clone());
                current = new_node;
            }
        }

        // Add to quick lookup index
        let hash_rel_path = HashRelativePath::new(
            current.borrow().source_archive.as_ref().unwrap().archive_hash,
            path_parts.to_vec(),
        );
        self.path_index.insert(hash_rel_path, current);
    }

    /// Look up a file by its HashRelativePath
    pub fn find_file(&self, hash_rel_path: &HashRelativePath) -> Option<Rc<RefCell<VirtualFileNode>>> {
        self.path_index.get(hash_rel_path).cloned()
    }
}