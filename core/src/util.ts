import { getLogLevel, getLogger } from "@davidsouther/jiffies/lib/esm/log.js";
export const LOGGER = getLogger("@ailly/core");

LOGGER.level = getLogLevel(process.env["AILLY_LOG_LEVEL"]);

export const isDefined = <T>(t: T | undefined): t is T => t !== undefined;
