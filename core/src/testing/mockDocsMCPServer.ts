import {
  McpServer,
  ResourceTemplate,
} from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

// Create an MCP server
const server = new McpServer({
  name: "mock-aws-docs",
  version: "1.0.0",
});

server.tool(
  "read_documentation",
  {
    url: z.string().describe("URL of the AWS documentation page to read"),
    max_length: z
      .number()
      .int()
      .optional()
      .describe("Maximum number of characters to return"),
    start_index: z
      .number()
      .int()
      .optional()
      .describe(
        "On return output starting at this character index, useful if a previous fetch was truncated and more content is required",
      ),
  },
  async ({ url, max_length, start_index }) => {
    if (
      !url.startsWith("https://docs.aws.amazon.com/") ||
      !url.endsWith(".html")
    ) {
      return {
        content: [
          {
            type: "text",
            text: "Error: URL must be from docs.aws.amazon.com domain and end with .html",
          },
        ],
        isError: true,
      };
    }
    // Generate mock content
    let content =
      "# S3 Bucket Naming Rules\n\nBucket names must be between 3 and 63 characters long.";

    if (start_index) content = content.substring(start_index);
    if (max_length) content = content.substring(0, max_length);

    return {
      content: [{ type: "text", text: content }],
    };
  },
);

server.tool(
  "search_documentation",
  {
    search_phrase: z.string().describe("Search phrase to use"),
    limit: z
      .number()
      .int()
      .optional()
      .describe("Maximum number of results to return"),
  },
  async ({ search_phrase, limit = 3 }) => {
    // Mock search results with the expected structure
    const mockResults = [
      {
        rank_order: 1,
        url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html",
        title: "Bucket naming rules - Amazon Simple Storage Service",
        context:
          "Rules for bucket naming in Amazon S3. Bucket names must be between 3 and 63 characters long and can only contain lowercase letters, numbers, dots, and hyphens.",
      },
      {
        rank_order: 2,
        url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/BucketRestrictions.html",
        title: "Bucket restrictions and limitations - Amazon S3",
        context:
          "Amazon S3 has certain limitations on bucket names, the number of buckets per account, and other restrictions that are important to understand.",
      },
      {
        rank_order: 3,
        url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingBucket.html",
        title: "Working with buckets - Amazon S3",
        context:
          "Learn about creating, configuring, and using Amazon S3 buckets for your object storage needs.",
      },
      {
        rank_order: 4,
        url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/creating-bucket.html",
        title: "Creating a bucket - Amazon S3",
        context:
          "Step-by-step guide to creating your first Amazon S3 bucket through the console or AWS CLI.",
      },
      {
        rank_order: 5,
        url: "https://docs.aws.amazon.com/AmazonS3/latest/API/API_CreateBucket.html",
        title: "CreateBucket - Amazon S3 API Reference",
        context:
          "API reference documentation for the CreateBucket operation in Amazon S3.",
      },
    ];

    // Filter results that might match the search phrase (for more realistic behavior)
    let filteredResults = mockResults;
    if (search_phrase.toLowerCase().includes("s3")) {
      //mock s3 search results
    } else if (search_phrase.toLowerCase().includes("lambda")) {
      //mock lambda search results
      filteredResults = [
        {
          rank_order: 1,
          url: "https://docs.aws.amazon.com/lambda/latest/dg/welcome.html",
          title: "What is AWS Lambda? - AWS Lambda",
          context:
            "AWS Lambda is a compute service that lets you run code without provisioning or managing servers.",
        },
        {
          rank_order: 2,
          url: "https://docs.aws.amazon.com/lambda/latest/dg/lambda-functions.html",
          title: "AWS Lambda functions - AWS Lambda",
          context:
            "Learn about Lambda functions, how they work, and how to use them in your applications.",
        },
        {
          rank_order: 3,
          url: "https://docs.aws.amazon.com/lambda/latest/dg/invocation-async.html",
          title: "Asynchronous invocation - AWS Lambda",
          context:
            "Invoking Lambda functions asynchronously and handling errors in asynchronous invocations.",
        },
      ];
    }
    const limitedResults = filteredResults.slice(0, limit);
    return {
      content: [
        {
          type: "text",
          text: 'Found ${limitedRsults.length} results for "${search_phrase}":',
          structuredContent: { results: limitedResults },
        },
      ],
    };
  },
);

server.tool(
  "recommend",
  {
    url: z
      .string()
      .describe("URL of the AWS documentation page to get recommendations for"),
  },
  async ({ url }) => {
    if (
      !url.startsWith("https://docs.aws.amazon.com/") ||
      !url.endsWith(".html")
    ) {
      return {
        content: [
          {
            type: "text",
            text: "Error: URL must be from docs.aws.amazon.com domain and end with .html",
          },
        ],
        isError: true,
      };
    }

    // Extract service name from URL for more realistic recommendations
    let serviceName = "Amazon S3"; // Default
    if (url.includes("/AmazonS3/")) {
      serviceName = "Amazon S3";
    } else if (url.includes("/lambda/")) {
      serviceName = "AWS Lambda";
    } else if (url.includes("/ec2/")) {
      serviceName = "Amazon EC2";
    }

    // Generate mock recommendations
    const recommendations = {
      highlyRated: [
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html",
          title: `${serviceName} User Guide`,
          context: `The official user guide for ${serviceName}.`,
        },
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/gettingstarted.html",
          title: `Getting Started with ${serviceName}`,
          context: "Step-by-step instructions for new users.",
        },
      ],
      new: [
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/storage-lens-optimize-cost.html",
          title: `${serviceName} Cost Optimization with Storage Lens`,
          context:
            "New feature to help optimize storage costs and usage patterns.",
        },
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-express-one-zone.html",
          title: `${serviceName} Express One Zone`,
          context:
            "New single-zone storage class for latency-sensitive applications.",
        },
      ],
      similar: [
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucket-policies.html",
          title: `${serviceName} Bucket Policies`,
          context: "Learn how to secure your resources with bucket policies.",
        },
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/access-control-overview.html",
          title: `${serviceName} Access Control Overview`,
          context: "Comprehensive guide to access control mechanisms.",
        },
      ],
      journey: [
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/monitoring-overview.html",
          title: `Monitoring ${serviceName}`,
          context: "How to monitor your resources and set up alerts.",
        },
        {
          url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/disaster-recovery-resiliency.html",
          title: `${serviceName} Disaster Recovery and Resiliency`,
          context: "Best practices for ensuring your data remains available.",
        },
      ],
    };

    return {
      content: [
        {
          type: "text",
          text: `Recommendations for ${url}`,
          structuredContent: {
            recommendations: {
              highlyRated: recommendations.highlyRated,
              new: recommendations.new,
              similar: recommendations.similar,
              journey: recommendations.journey,
            },
          },
        },
      ],
    };
  },
);

// Add a dynamic greeting resource
server.resource(
  "greeting",
  new ResourceTemplate("greeting://{name}", { list: undefined }),
  async (uri, { name }) => ({
    contents: [
      {
        uri: uri.href,
        text: `Hello, ${name}!`,
      },
    ],
  }),
);

async function startServer() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

startServer();
