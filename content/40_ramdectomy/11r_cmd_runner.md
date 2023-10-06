---
generated: 2023-10-05T20:10:28.455Z
debug: {"id":"chatcmpl-86OwiLebQpNh2MdQYvYZcweYtVOJE","model":"gpt-4-0613","usage":{"prompt_tokens":410,"completion_tokens":696,"total_tokens":1106},"finish":"stop"}
---

Ramda's "prop" function provides a simple way to access dynamic properties in objects. It's equivalent in native Javascript can be accomplished with function that accepts the wanted "prop" as an argument, and returns a function that takes in an object and returns its prop if it exists. 

Ramda's "invoker" is a function that, when called with an object and (optionally) some arguments, calls the specified method on that object with the provided arguments. In native JavaScript, this can be achieved by creating a function that takes in (optionally) a number of arguments, and then returns a function that, when called with an object, invokes that object's method with the arguments.

Ramda's "cond" function is similar to "switch-case". It can be replaced with a native JavaScript function for convenience.

Ramda's function "startsWith" can be replace with the native function in JavaScript.

"Compose", "split" and "trim" can be replaced by native JavaScript equivalents.

"Apply" and "T" are specific to Ramda and need created workarounds.

The refactored file will have most of the Ramda replaced with native JavaScript:

```javascript
/**
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
import { T, apply } from "ramda"; // These are specific to Ramda

import { flipApplyMap } from "./ext-ramda.js";
import { log } from "./utils/util-log.js";

const readerProp = (reader) => obj => obj && obj[reader];
const handlersProp = (handlers) => obj => obj && obj[handlers];

const close = function() {
  return function(obj) {
    if(obj && obj.close) {
      return obj.close();
    }
  };
};

const on = function(eventName, listener) {
  return function(obj) {
    if(obj && obj.on) {
      obj.on(eventName, listener);
    }
  };
};

const processCommands = (context) => {
  const commands = handlersProp(context);
  const reader = readerProp(context);

  return function(command) {
    for(let i = 0; i < commands.length; i++) {
      if(command.startsWith(commands[i][0])) {
        return commands[i][1]();
      }
    }
  
    if(command.startsWith('quit')) {
      return close(reader);
    }
  
    return log("Command not recognized.");
  };
};

const getCommands = (command) => command.trim().split(' ');

const handleInput = (context) => function(command) {
  const commands = getCommands(command);
  return processCommands(context)(commands);
};

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
  const inputs = flipApplyMap([handleInput, readerProp(context)])(context);
  return function(callback) {
    inputs.map(input => on('line', callback)(input));
  };
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