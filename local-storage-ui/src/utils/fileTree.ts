import { StoredFile } from '../types/api';

export interface FolderItem {
  type: 'folder';
  name: string;
  path: string;
  fileCount: number;
  totalSize: number;
}

export interface FileEntry {
  type: 'file';
  name: string;
  path: string;
  file: StoredFile;
}

export type SortBy = 'name' | 'size' | 'date';
export type SortOrder = 'asc' | 'desc';

/**
 * Splits a flat list of files (the backend has no real folder objects, only keys like
 * "a/b/c.txt") into the immediate children of `currentPath`: one level of subfolders plus any
 * files directly at this level.
 */
export function buildFileTree(files: StoredFile[], currentPath: string): { folders: FolderItem[]; files: FileEntry[] } {
  const prefix = currentPath ? `${currentPath}/` : '';
  const folders = new Map<string, FolderItem>();
  const fileEntries: FileEntry[] = [];

  for (const file of files) {
    if (!file.key.startsWith(prefix)) continue;
    const relative = file.key.slice(prefix.length);
    if (!relative) continue;

    const slashIndex = relative.indexOf('/');
    if (slashIndex === -1) {
      fileEntries.push({ type: 'file', name: relative, path: file.key, file });
      continue;
    }

    const folderName = relative.slice(0, slashIndex);
    const folderPath = prefix + folderName;
    const existing = folders.get(folderPath);
    if (existing) {
      existing.fileCount += 1;
      existing.totalSize += file.file_size;
    } else {
      folders.set(folderPath, { type: 'folder', name: folderName, path: folderPath, fileCount: 1, totalSize: file.file_size });
    }
  }

  return { folders: Array.from(folders.values()), files: fileEntries };
}

export function sortItems<T extends FolderItem | FileEntry>(items: T[], sortBy: SortBy, sortOrder: SortOrder): T[] {
  const sorted = [...items].sort((a, b) => {
    let cmp = 0;
    switch (sortBy) {
      case 'name':
        cmp = a.name.localeCompare(b.name);
        break;
      case 'size':
        cmp = (a.type === 'folder' ? a.totalSize : a.file.file_size) - (b.type === 'folder' ? b.totalSize : b.file.file_size);
        break;
      case 'date':
        cmp = (a.type === 'file' ? new Date(a.file.upload_time).getTime() : 0)
          - (b.type === 'file' ? new Date(b.file.upload_time).getTime() : 0);
        break;
    }
    return sortOrder === 'asc' ? cmp : -cmp;
  });
  return sorted;
}

export function breadcrumbs(currentPath: string): { name: string; path: string }[] {
  const parts = currentPath.split('/').filter(Boolean);
  return [
    { name: 'Home', path: '' },
    ...parts.map((part, index) => ({ name: part, path: parts.slice(0, index + 1).join('/') })),
  ];
}
