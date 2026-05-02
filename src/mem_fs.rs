//! Test helpers shared across `content` submodules.
//!
//! `mem_fs!` builds a `vfs::MemoryFS` from a literal record, mirroring the
//! TypeScript `ObjectFileSystemAdapter` style: `"name": { ... }` is a
//! directory, `"name": "content"` is a (text) file. Returns the filesystem
//! root as a `VfsPath`.

#[macro_export]
macro_rules! mem_fs_node {
    ($parent:expr, $name:literal, { $($k:literal : $v:tt),* $(,)? }) => {{
        let dir = $parent.join($name).unwrap();
        dir.create_dir().unwrap();
        $($crate::mem_fs_node!(&dir, $k, $v);)*
    }};
    ($parent:expr, $name:literal, $content:literal) => {{
        use std::io::Write;
        let f = $parent.join($name).unwrap();
        write!(f.create_file().unwrap(), "{}", $content).unwrap();
    }};
}

#[macro_export]
macro_rules! mem_fs {
    ($($name:literal : $value:tt),* $(,)?) => {{
        let root: ::vfs::VfsPath = ::vfs::VfsPath::new(::vfs::MemoryFS::new());
        $($crate::mem_fs_node!(&root, $name, $value);)*
        root
    }};
}
