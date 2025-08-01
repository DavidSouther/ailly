import { Err, Ok } from "@davidsouther/jiffies/lib/esm/result.js";

const MockClient = {
  tools: [
    {
      name: "add",
      description: "Add an array of numbers together.",
      inputSchema: {
        type: "object",
        properties: { args: { type: "array" } },
      },
    },
  ],

  invokeTool: async (toolName, parameters, _context) => {
    if (toolName === "add") {
      const { args } = parameters;
      const nums = args.map(Number);
      const sum = nums.reduce((a, b) => a + b, 0);
      return Ok({ type: "text", text: `${sum}` });
    }
    return Err({ message: "unknown tool" });
  },
};

export default MockClient;
