/** POSIX path helpers for the relative paths scene documents use. */

/** Directory part of `path` (`""` for a bare file name). */
export function dirname(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

/** Resolve `path` against directory `dir`, folding `.` and `..`. A leading `..` that climbs
 * past the root is kept (the result is then outside the source). */
export function join(dir: string, path: string): string {
  const parts: string[] = [];
  for (const part of `${dir}/${path}`.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." && parts.length > 0 && parts.at(-1) !== "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}
