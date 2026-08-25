/**
 * @param {string} left
 * @param {string} right
 */
export function samePath(left, right) {
  /** @param {string} value */
  const normalize = (value) => {
    let normalized = value.trim().replaceAll("\\", "/");
    if (/^\/\/\?\/unc\//i.test(normalized)) {
      normalized = `//${normalized.slice(8)}`;
    } else if (/^\/\/\?\//.test(normalized)) {
      normalized = normalized.slice(4);
    }

    const segments = [];
    for (const segment of normalized.split("/")) {
      if (!segment || segment === ".") continue;
      if (segment === ".." && segments.length > 1) {
        segments.pop();
        continue;
      }
      segments.push(segment);
    }
    return segments.join("/").replace(/\/+$/, "").toLowerCase();
  };

  return normalize(left) === normalize(right);
}
