#!/usr/bin/env node

import { expect, test } from "vitest";
import { type MCPServersConfig, type ToolInformation, mcpWrapper } from "./mcp";

test("mock-doc-test", async () => {
  console.log("Initializing MCP wrapper...");

  const mcpConfig: MCPServersConfig = {
    servers: {
      "mock-aws-docs": {
        type: "stdio",
        command: "node",
        args: ["./lib/testing/mockDocsMCPServer.js"],
        env: {
          FASTMCP_LOG_LEVEL: "ERROR",
        },
      },
    },
  };

  await mcpWrapper.initialize(mcpConfig);

  //test get tools
  const tools: Map<string, ToolInformation> = await mcpWrapper.getToolsMap();

  expect(tools.size).toBe(3);
  expect(tools.has("read_documentation")).toBe(true);
  expect(tools.has("search_documentation")).toBe(true);
  expect(tools.has("recommend")).toBe(true);

  //test read_documentation
  const docResult = await mcpWrapper.invokeTool("read_documentation", {
    url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html",
  });
  const textContent = docResult.content[0].text;
  expect(textContent.toLowerCase()).toContain(
    "bucket names must be between 3 and 63",
  );

  //test search_documentation with limit
  const s3SearchResult = await mcpWrapper.invokeTool("search_documentation", {
    search_phrase: "s3 bucket naming",
    limit: 2,
  });
  const s3Content = s3SearchResult.content[0].structuredContent.results;
  expect(s3Content.length).toBe(2);
  expect(s3Content[0].title.toLowerCase()).toContain("naming rules"); //rank 1
  expect(s3Content[1].title.toLowerCase()).toContain(
    "restrictions and limitations",
  ); //rank 2

  //test search_documentation with no limit
  const s3SearchResultNoLimit = await mcpWrapper.invokeTool(
    "search_documentation",
    {
      search_phrase: "s3 bucket naming",
    },
  );
  const s3ContentNoLimit =
    s3SearchResultNoLimit.content[0].structuredContent.results;
  expect(s3ContentNoLimit.length).toBe(3);

  //test search_documentation lambda search
  const lambdaSearchResult = await mcpWrapper.invokeTool(
    "search_documentation",
    {
      search_phrase: "lambda",
    },
  );
  const lambdaSearchContent =
    lambdaSearchResult.content[0].structuredContent.results;
  expect(lambdaSearchContent.length).toBe(3);
  expect(lambdaSearchContent[0].title.toLowerCase()).toContain(
    "what is aws lambda",
  );

  //test recommend
  const recommendResults = await mcpWrapper.invokeTool("recommend", {
    url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html",
  });
  const recommendations =
    recommendResults.content[0].structuredContent.recommendations;

  expect(recommendations.highlyRated.length).toBe(2);
  expect(recommendations.highlyRated[0].title.toLowerCase()).toContain(
    "s3 user guide",
  );
  expect(recommendations.new.length).toBe(2);
  expect(recommendations.new[0].title.toLowerCase()).toContain(
    "s3 cost optimization",
  );
  expect(recommendations.similar.length).toBe(2);
  expect(recommendations.similar[0].title.toLowerCase()).toContain(
    "s3 bucket policies",
  );
  expect(recommendations.journey.length).toBe(2);
  expect(recommendations.journey[0].title.toLowerCase()).toContain(
    "monitoring amazon s3",
  );

  await mcpWrapper.cleanup();
});
