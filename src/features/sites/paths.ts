/** Document root shown relative to the project folder ("." for the folder itself). */
export function relativeDocRoot(projectRoot: string, documentRoot: string): string {
  const root = projectRoot.replace(/[\\/]+$/, "");
  const doc = documentRoot.replace(/[\\/]+$/, "");
  if (doc.toLowerCase() === root.toLowerCase()) return ".";
  const prefix = `${root.toLowerCase()}\\`;
  if (doc.toLowerCase().startsWith(prefix) || doc.toLowerCase().startsWith(`${root.toLowerCase()}/`)) {
    return doc.slice(root.length + 1);
  }
  return documentRoot;
}

export function folderName(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
}
