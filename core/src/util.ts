export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;

interface PromiseWithResolvers<T> {
  resolve: (t: T | PromiseLike<T>) => void;
  reject: (reason?: any) => void;
  promise: Promise<T>;
}

declare global {
  interface PromiseConstructor {
    withResolvers<T>(): PromiseWithResolvers<T>;
  }
}

Promise.withResolvers =
  Promise.withResolvers ??
  function makePromise<T = void>(): PromiseWithResolvers<T> {
    let resolve: (t: T | PromiseLike<T>) => void = () => {};
    let reject: (reason?: any) => void = () => {};
    const promise = new Promise<T>((r, j) => {
      resolve = r;
      reject = j;
    });
    return { promise, resolve, reject };
  };
