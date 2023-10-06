---
generated: 2023-10-05T20:24:51.843Z
debug: {"id":"chatcmpl-86PBKnwzNoKgrzUEhWK5jnMdkdgkH","model":"gpt-4-0613","usage":{"prompt_tokens":833,"completion_tokens":372,"total_tokens":1205},"finish":"stop"}
---

```javascript
/**
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
import { flipApplyMap } from "./ext-ramda.js";
import { log } from "./utils/util-log.js";

const readerProp = context => context["reader"];
const handlersProp = context => context["handlers"];

const close = receiver => receiver["close"]();
const on = (event, handle) => receiver => receiver["on"](event, handle);

const processCommands = (context) => {
  return (command) => {
    const handlers = handlersProp(context);

    for (let i = 0; i < handlers.length; i++) {
      const [condition, handler] = handlers[i];

      if (condition(command)) {
        return handler(command);
      }
    }

    if (command.startsWith("quit")) {
      return close(readerProp(context));
    }

    return log("Command not recognized.");
  };
};

const getCommands = input => input.trim().split(" ");

const handleInput = (context) => (input) => processCommands(context)(getCommands(input));

/**
 *
 * @param {{
 *  reader: { on: (event: string) => void, close: () => void },
 *  handlers?: [(a: any) => boolean, (a: any) => any][]
 *  commands?: string[]
 * }} context
 * @returns
 */
const readCommands = (context) => {
  const inputHandler = handleInput(context);
  const reader = readerProp(context);

  reader.on("line", (input) => inputHandler(input));
};

export {
  readCommands,
  readerProp,
  handlersProp,
  close,
  on,
  processCommands,
  getCommands,
  handleInput,
};
```