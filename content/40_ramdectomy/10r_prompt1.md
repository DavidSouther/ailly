---
generated: 2023-10-05T20:04:37.402Z
debug: {"id":"chatcmpl-86Or3SjxDYAvMlZ2bozLIhmq9wVXo","model":"gpt-4-0613","usage":{"prompt_tokens":872,"completion_tokens":813,"total_tokens":1685},"finish":"stop"}
---

```
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
  [(cmd) => cmd.startsWith("help"), () => { log(help); }],
  [(cmd) => cmd.startsWith("create-user-pool"), createUserPoolHandler],
  [(cmd) => cmd.startsWith("sign-up"), signUpHandler],
  [(cmd) => cmd.startsWith("resend-confirmation-code"), resendConfirmationCodeHandler],
  [(cmd) => cmd.startsWith("confirm-sign-up"), confirmSignUpHandler],
  [(cmd) => cmd.startsWith("list-users"), listUsersHandler],
  [(cmd) => cmd.startsWith("admin-get-user"), adminGetUserHandler],
  [(cmd) => cmd.startsWith("admin-initiate-auth"), adminInitiateAuthHandler],
  [(cmd) => cmd.startsWith("verify-software-token"), verifySoftwareTokenHandler],
  [(cmd) => cmd.startsWith("admin-respond-to-auth-challenge"), adminRespondToAuthChallengeHandler],
  [(cmd) => cmd.startsWith("clean-up"), cleanUpHandler],
];

cmdInterface.write('Welcome to Amazon Cognito. Type "help" for more info.\n');
readCommands({ reader: cmdInterface, handlers });
```