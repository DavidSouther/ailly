---
generated: 2023-10-05T20:19:00.278Z
debug: {"id":"chatcmpl-86P4gNCxy0nvJc56NL9sythI8uzgU","model":"gpt-4-0613","usage":{"prompt_tokens":1609,"completion_tokens":857,"total_tokens":2466},"finish":"stop"}
---

```javascript
/**
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
import { createInterface } from "readline";
import { stdin as input, stdout as output } from "process";

import { readCommands } from "libs/cmd-runner.js";
import { log } from "libs/utils/util-log.js";
import { createUserPoolHandler } from "./command-handlers/create-user-pool-handler.js";
import { cleanUpHandler } from "./command-handlers/clean-up-handler.js";
import { signUpHandler } from "./command-handlers/sign-up-handler.js";
import { confirmSignUpHandler } from "./command-handlers/confirm-sign-up-handler.js";
import { adminInitiateAuthHandler } from "./command-handlers/admin-initiate-auth-handler.js";
import { verifySoftwareTokenHandler } from "./command-handlers/verify-software-token-handler.js";
import { adminRespondToAuthChallengeHandler } from "./command-handlers/admin-respond-to-auth-challenge-handler.js";
import { adminGetUserHandler } from "./command-handlers/admin-get-user-handler.js";
import { listUsersHandler } from "./command-handlers/list-users-handler.js";
import { resendConfirmationCodeHandler } from "./command-handlers/resend-confirmation-code-handler.js";

const cmdInterface = createInterface({ input, output });

const help = `
 usage: <command> [<args>]

   Commands:

   help                                   Shows this list of commands.

   create-user-pool <user_pool_name>      Creates and configures an Amazon Cognito user pool
                                          compatible with this example.

   sign-up <user_name> <password>         Creates a new user in the user pool.
    <email>                               Also emails a verification code to
                                          the provided email address.

   resend-confirmation-code <user_name>   Sends the user a new confirmation code.

   confirm-sign-up <user_name> <code>     Confirms a new user with the code received
                                          from the 'sign-up' step.

   list-users                             Lists all the users in the active user pool.

   admin-get-user <user_name>             Attempts to get a user from the active user pool.

   admin-initiate-auth <user_name>        Attempts to authenticate to the user pool
    <password>                            with the provided credentials. This command
                                          requires AWS developer credentials to be set
                                          up. initiate-auth does not.

   verify-software-token <totp>           Attempts to verify a time-based one-time password.
                                          This completes MFA setup.

   admin-respond-to-auth-challenge        Attempts to verify the time-based one-time password.
    <username> <totp>                     This is the last step in authentication.

   clean-up                               Deletes any user pool created with this
                                          tool.

   quit                                   Quits the current interactive session.
 `;

const handlers = [
  [input => input.startsWith("help"), () => log(help)],
  [input => input.startsWith("create-user-pool"), createUserPoolHandler],
  [input => input.startsWith("sign-up"), signUpHandler],
  [input => input.startsWith("resend-confirmation-code"), resendConfirmationCodeHandler],
  [input => input.startsWith("confirm-sign-up"), confirmSignUpHandler],
  [input => input.startsWith("list-users"), listUsersHandler],
  [input => input.startsWith("admin-get-user"), adminGetUserHandler],
  [input => input.startsWith("admin-initiate-auth"), adminInitiateAuthHandler],
  [input => input.startsWith("verify-software-token"), verifySoftwareTokenHandler],
  [input => input.startsWith("admin-respond-to-auth-challenge"), adminRespondToAuthChallengeHandler],
  [input => input.startsWith("clean-up"), cleanUpHandler],
];

cmdInterface.write('Welcome to Amazon Cognito. Type "help" for more info.\n');
readCommands({ reader: cmdInterface, handlers });
```
The Ramda `startsWith` function has been replaced with the native JavaScript `startsWith` string method. Now, instead of passing a command string array to the Ramda `startsWith` function, we instead use a function that checks if the input starts with a specified command string.