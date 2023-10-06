---
generated: 2023-10-05T20:04:10.298Z
debug: {"id":"chatcmpl-86Or3tv7pyToq65KbGeEbied2DWqc","model":"gpt-4-0613","usage":{"prompt_tokens":550,"completion_tokens":529,"total_tokens":1079},"finish":"stop"}
---

Your code uses Ramda's `join` function. The JavaScript equivalent to Ramda's `join` function is Array's `join` method.

Here's the refactored code:

```javascript
/**
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

import { log } from "libs/utils/util-log.js";
import { getTmp, setTmp } from "libs/utils/util-fs.js";
import { createUserPool } from "../../../actions/create-user-pool.js";
import { FILE_USER_POOLS, NAME_CLIENT } from "./constants.js";
import { setUserPoolMfaConfig } from "../../../actions/set-user-pool-mfa-config.js";
import { createUserPoolClient } from "../../../actions/create-user-pool-client.js";

const storeUserPoolMeta = (...args) => {
  const tmp = getTmp(FILE_USER_POOLS);
  const entry = args.join(",");
  setTmp(FILE_USER_POOLS, tmp ? `${tmp}\n${entry}` : entry);
};

const validateUserPool = (poolName) => {
  if (!poolName) {
    throw new Error(
      `User pool name is missing. It must be provided as an argument to the 'initialize' command.`,
    );
  }

  const tmp = getTmp(FILE_USER_POOLS);

  if (tmp) {
    throw new Error(
      `A user pool already exists. Run 'clean-up' to delete any existing user pools created with this tool.`,
    );
  }
};

const createUserPoolHandler = async (commands) => {
  const [_, poolName] = commands;

  try {
    validateUserPool(poolName);

    log(`Creating user pool: ${poolName}`);

    const {
      UserPool: { Id },
    } = await createUserPool(poolName);
    log(`User pool created.`);

    log("Configuring user pool to only allow MFA via an authenticator app.");
    await setUserPoolMfaConfig(Id);
    log("MFA configured.");

    log(`Creating user pool client: ${NAME_CLIENT}`);
    const {
      UserPoolClient: { ClientId },
    } = await createUserPoolClient(NAME_CLIENT, Id);
    log(`Client created.`);

    storeUserPoolMeta(Id, ClientId, poolName);
  } catch (err) {
    log(err);
  }
};

export { createUserPoolHandler };
```

The only line that changed was the following:

From:
```javascript
const entry = join(",", args);
```
To:
```javascript
const entry = args.join(",");
```