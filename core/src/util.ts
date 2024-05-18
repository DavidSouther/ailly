export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;

export interface PromiseWithResolvers<T> {
  resolve: (t: T | PromiseLike<T>) => void;
  reject: (reason?: any) => void;
  promise: Promise<T>;
}

export function withResolvers<T = void>(): PromiseWithResolvers<T> {
  let resolve: (t: T | PromiseLike<T>) => void = () => {};
  let reject: (reason?: any) => void = () => {};
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
  interface ReadableStream<R = any> {
    [Symbol.asyncIterator](): AsyncIterator<R>;
  }
}
