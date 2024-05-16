import { getLogLevel, getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
export const LOGGER = getLogger("@ailly/core");

LOGGER.level = getLogLevel(process.env["AILLY_LOG_LEVEL"]);

export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;

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
