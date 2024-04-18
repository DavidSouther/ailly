import { LEVEL, getLogger } from "@davidsouther/jiffies/lib/esm/log.js";
export const LOGGER = getLogger("@ailly/core");

LOGGER.level = getLogLevel(process.env["AILLY_LOG_LEVEL"]);

export function getLogLevel(
  level?: string | undefined,
  verbose = false,
  isPipe = false
) {
  if (level) {
    switch (level.toLowerCase()) {
      case "debug":
        return LEVEL.DEBUG;
      case "info":
        return LEVEL.INFO;
      case "warn":
        return LEVEL.WARN;
      case "error":
        return LEVEL.ERROR;
      default:
        if (!isNaN(+level)) return Number(level);
    }
  }
  if (verbose) {
    return LEVEL.DEBUG;
  }
  if (isPipe) {
    return LEVEL.SILENT;
  }
  return LEVEL.INFO;
}

export function prettyLogFormatter(data: {
  name: string;
  prefix: string;
  level: number;
  message: string;
  source: string;
}): string {
  return `${data.prefix}: ${data.message}`;
}

export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;
