---
generated: 2023-10-05T20:03:43.201Z
debug: {"id":"chatcmpl-86Or3RAUSjk66IbHgtTQ8qBPjHyqm","model":"gpt-4-0613","usage":{"prompt_tokens":226,"completion_tokens":236,"total_tokens":462},"finish":"stop"}
---

Below is the refactored code using native JavaScript instead of ramda's `pipe`, `zipWith`, and `fromPairs`:

```javascript
import { Item } from "src/types/item.js";

const makePairs = (key, value) => {
  if (key === "archived") {
    return [key, Boolean(value.longValue)];
  } else {
    return [key, value.stringValue];
  }
};
  
const parseItem = (record) => {
  const keys = ["id", "description", "guide", "status", "name", "archived"];
  
  const pairs = keys.map((key, index) => makePairs(key, record[index]));

  return pairs.reduce((acc, [key, value]) => ({ ...acc, [key]: value }), {});
};

export { parseItem };
```

The `pipe` and `zipWith` operations are replaced by `map` function, which iterates over the keys and uses the `makePairs` function to map each key with corresponding value. `fromPairs` is replaced with a `reduce` operation, which transforms the generated array of pairs into an object.