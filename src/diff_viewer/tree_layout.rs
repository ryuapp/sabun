use super::row_geometry::{cumulative_offsets, row_index_at_position};
use super::{DiffFile, FileTreeNode, FileTreeRow, HashSet, Pixels, SharedString, px};

enum OrderedTreeEntry<'a> {
    Directory(&'a str, &'a FileTreeNode),
    File(usize),
}

impl OrderedTreeEntry<'_> {
    fn first_file_index(&self) -> usize {
        match self {
            Self::Directory(_, node) => node.first_file_index.unwrap_or(usize::MAX),
            Self::File(file_index) => *file_index,
        }
    }
}

pub(super) struct FileTreeData {
    pub(super) rows: Vec<FileTreeRow>,
    pub(super) offsets: Vec<Pixels>,
}

pub(super) fn build_file_tree_data(
    files: &[DiffFile],
    collapsed_directories: &HashSet<String>,
) -> FileTreeData {
    let rows = build_file_tree_rows(files, collapsed_directories);
    let offsets = file_tree_row_offsets(&rows);
    FileTreeData { rows, offsets }
}

pub(super) fn normalized_path_components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect()
}

pub(super) fn build_file_tree_rows(
    files: &[DiffFile],
    collapsed_directories: &HashSet<String>,
) -> Vec<FileTreeRow> {
    let mut root = FileTreeNode::default();
    for (file_index, file) in files.iter().enumerate() {
        let components = normalized_path_components(file.display_path());
        let Some((file_name, directories)) = components.split_last() else {
            continue;
        };
        let mut node = &mut root;
        node.first_file_index.get_or_insert(file_index);
        for directory in directories {
            node = node.directories.entry((*directory).to_owned()).or_default();
            node.first_file_index.get_or_insert(file_index);
        }
        node.files.push(((*file_name).to_owned(), file_index));
    }

    let mut rows = Vec::new();
    flatten_file_tree(&root, "", 0, collapsed_directories, &mut rows);
    rows
}

pub(super) fn flatten_file_tree(
    node: &FileTreeNode,
    parent_path: &str,
    depth: usize,
    collapsed_directories: &HashSet<String>,
    rows: &mut Vec<FileTreeRow>,
) {
    let mut entries = node
        .directories
        .iter()
        .map(|(name, child)| OrderedTreeEntry::Directory(name, child))
        .chain(
            node.files
                .iter()
                .map(|(_, file_index)| OrderedTreeEntry::File(*file_index)),
        )
        .collect::<Vec<_>>();
    entries.sort_by_key(OrderedTreeEntry::first_file_index);

    for entry in entries {
        match entry {
            OrderedTreeEntry::Directory(name, child) => {
                let mut path = if parent_path.is_empty() {
                    name.to_owned()
                } else {
                    format!("{parent_path}/{name}")
                };
                let mut display_name = name.to_owned();
                let mut display_node = child;

                while display_node.files.is_empty()
                    && display_node.directories.len() == 1
                    && !collapsed_directories.contains(&path)
                {
                    let Some((child_name, child_node)) = display_node.directories.first_key_value()
                    else {
                        break;
                    };
                    display_name.push('/');
                    display_name.push_str(child_name);
                    path.push('/');
                    path.push_str(child_name);
                    display_node = child_node;
                }

                let expanded = !collapsed_directories.contains(&path);
                rows.push(FileTreeRow::Directory {
                    path: SharedString::from(path.clone()),
                    name: SharedString::from(display_name),
                    depth,
                    expanded,
                });
                if expanded {
                    flatten_file_tree(display_node, &path, depth + 1, collapsed_directories, rows);
                }
            }
            OrderedTreeEntry::File(file_index) => {
                rows.push(FileTreeRow::File { file_index, depth });
            }
        }
    }
}

pub(super) fn file_tree_row_offsets(rows: &[FileTreeRow]) -> Vec<Pixels> {
    cumulative_offsets(rows, FileTreeRow::height)
}

pub(super) fn sticky_file_tree_directories(
    rows: &[FileTreeRow],
    offsets: &[Pixels],
    scroll_position: Pixels,
) -> Vec<FileTreeRow> {
    if scroll_position <= px(0.) || rows.is_empty() {
        return Vec::new();
    }

    let active_index = row_index_at_position(offsets, scroll_position, rows.len());
    let depth_count = match &rows[active_index] {
        FileTreeRow::Directory { depth, .. } => depth + 1,
        FileTreeRow::File { depth, .. } => *depth,
    };
    let mut directories = vec![None; depth_count];
    let mut remaining = depth_count;
    for row in rows[..=active_index].iter().rev() {
        let FileTreeRow::Directory { depth, .. } = row else {
            continue;
        };
        if *depth < directories.len() && directories[*depth].is_none() {
            directories[*depth] = Some(row.clone());
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }
    directories.into_iter().flatten().collect()
}
