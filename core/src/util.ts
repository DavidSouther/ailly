export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;

export const promiseTimeout = (sleep: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, sleep));

export interface PromiseWithResolvers<T> {
  resolve: (t: T | PromiseLike<T>) => void;
  reject: (reason?: unknown) => void;
  promise: Promise<T>;
}

export function withResolvers<T = void>(): PromiseWithResolvers<T> {
  let resolve: (t: T | PromiseLike<T>) => void = () => {};
  let reject: (reason?: unknown) => void = () => {};
  const promise = new Promise<T>((r, j) => {
    resolve = r;
    reject = j;
  });
  return { promise, resolve, reject };
}

export function withResolved<T = void>(t: T): PromiseWithResolvers<T> {
  const resolvers = withResolvers<T>();
  resolvers.resolve(t);
  return resolvers;
}

declare global {
  interface ReadableStream<R> {
    [Symbol.asyncIterator](): AsyncIterator<R>;
  }
}
