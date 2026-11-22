/** List edits for the folder and ignore lists, and the save queue. */

/** `path` appended unless it is already there or the picker was cancelled (null). */
export const addPath = (list: string[], path: string | null): string[] => (path && !list.includes(path) ? [...list, path] : list);

export const removePath = (list: string[], path: string): string[] => list.filter((p) => p !== path);

/** Runs tasks one after another in call order. Each change is saved as it happens, so two quick ones must not reach
 * the backend out of order; a failed save does not stop the ones after it. */
export function serial() {
  let tail: Promise<unknown> = Promise.resolve();
  return <T>(task: () => Promise<T>): Promise<T> => {
    const run = tail.then(task);
    tail = run.catch(() => {});
    return run;
  };
}
