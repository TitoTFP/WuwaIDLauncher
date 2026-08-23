/**
 * @param {number} timeoutMs
 * @returns {{ promise: Promise<void>, resolve: () => void }}
 */
export function createPatchStatusWaiter(timeoutMs) {
  let settled = false;
  /** @type {ReturnType<typeof setTimeout>} */
  let timer;
  /** @type {() => void} */
  let resolveWaiter = () => {};

  /** @type {Promise<void>} */
  const promise = new Promise((resolve, reject) => {
    timer = setTimeout(() => {
      settled = true;
      reject(new Error("Waktu tunggu status patch habis."));
    }, timeoutMs);
    resolveWaiter = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve();
    };
  });

  return { promise, resolve: () => resolveWaiter() };
}
