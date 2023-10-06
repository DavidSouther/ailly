---
generated: 2023-10-05T20:04:07.675Z
debug: {"id":"chatcmpl-86Or3Gtk9dwvx2OtKkjZgpmPxfE3y","model":"gpt-4-0613","usage":{"prompt_tokens":617,"completion_tokens":534,"total_tokens":1151},"finish":"stop"}
---

```javascript
/**
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
import { readdirSync } from "fs";
import { detachRolePolicy } from "../../../../iam/actions/detach-role-policy.js";
import { deleteRole } from "../../../../iam/actions/delete-role.js";
import { concatMap, promiseAll } from "../../../../libs/ext-ramda.js";
import { log } from "../../../../libs/utils/util-log.js";
import {
  deleteFiles,
  dirnameFromMetaUrl,
} from "../../../../libs/utils/util-fs.js";
import { deleteFunction } from "../../../actions/delete-function.js";
import {
  ARN_POLICY_LAMBDA_BASIC_EXECUTION,
  NAME_ROLE_LAMBDA,
} from "./constants.js";

const dirname = dirnameFromMetaUrl(import.meta.url);

const getFilesByExt = (ext) => 
  readdirSync.filter((fileName) => fileName.includes(ext));

const getZipFiles = getFilesByExt(".zip");
const getTmpFiles = getFilesByExt(".tmp");

const cleanUpFunctions = (functions) =>
  promiseAll(functions.map((func) => deleteFunction(func).catch(log)));

const cleanUpZipFiles = deleteFiles(
  concatMap(`${dirname}../../../functions/`)
);

const cleanUpTmpFiles = deleteFiles(concatMap("./"));

const cleanUpRolePolicy = (roleName, policyArn) =>
  detachRolePolicy(roleName, policyArn).catch(log);

const cleanUpRole = (roleName) => deleteRole(roleName).catch(log);

const cleanUpHandler = async () => {
  const zippedFuncs = getZipFiles(`${dirname}../../../functions/`);
  const tmpFiles = getTmpFiles("./");
  const funcNames = zippedFuncs.map((zippedFunc) => zippedFunc.split(".zip")[0]);

  try {
    log("Tidying up 🧹");

    log("Deleting Lambda functions.");
    await cleanUpFunctions(funcNames);

    log("Deleting local zipped versions of functions.");
    cleanUpZipFiles(zippedFuncs);

    log("Removing policy from role created during initialization.");
    await cleanUpRolePolicy(
      NAME_ROLE_LAMBDA,
      ARN_POLICY_LAMBDA_BASIC_EXECUTION
    );

    log("Deleting role created during initialization.");
    await cleanUpRole(NAME_ROLE_LAMBDA);

    log("Removing temporary files.");
    cleanUpTmpFiles(tmpFiles);

    log("All done ✨.");
  } catch (err) {
    log(err);
  }
};

export { cleanUpHandler };
```
