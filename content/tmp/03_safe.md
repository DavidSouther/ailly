---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-05-10T09:53:14.929Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
isolated: true
prompt: |
  Create a similar function that uses a list of "safe" text files that we should
  always assume is good to sent, including common text authoring, config, and
  programming languages.
---
# Text File Extension Whitelist Function

Here's an implementation that focuses on a comprehensive whitelist approach, identifying files that are known to be text-based. This is often more reliable than binary detection for certain use cases:

```typescript
/**
 * Determines if a file is a known text file type based on its extension and/or content
 *
 * @param path The file path to examine
 * @param content The file content (optional, used for additional verification if needed)
 * @param options Optional configuration parameters
 * @returns boolean - true if the file is a known text type, false otherwise
 */
export function isKnownTextFile(
  path: string,
  content?: string,
  options: {
    fallbackToContentCheck?: boolean; // If true, files without extensions will be analyzed
    customTextExtensions?: string[]; // Additional text extensions beyond defaults
    strictMode?: boolean; // If true, only accepts files with whitelisted extensions
  } = {}
): boolean {
  // Configure options with defaults
  const {
    fallbackToContentCheck = true,
    strictMode = false,
    customTextExtensions = [],
  } = options;

  // Extract extension (converting to lowercase)
  const extension = path
    .substring(path.lastIndexOf("."))
    .toLowerCase();

  // Comprehensive list of known text file extensions
  // biome-ignore format: This is a large array better represented as a table
  const textFileExtensions = [
    // Programming Languages
    ".js", ".ts", ".jsx", ".tsx", ".py", ".java", ".c", ".cpp", ".cs", ".go", ".rs", 
    ".rb", ".php", ".pl", ".pm", ".scala", ".sc", ".clj", ".cljs", ".cljc", ".edn", 
    ".coffee", ".elm", ".erl", ".ex", ".exs", ".fs", ".fsx", ".groovy", ".hs", ".kt", 
    ".kts", ".lua", ".m", ".mm", ".swift", ".vb", ".pas", ".pp", ".nim", ".r",
    ".dart", ".d", ".f", ".f90", ".jl", ".lisp", ".ml", ".re", ".sh", ".bash", ".zsh",
    
    // Markup/Web
    ".html", ".htm", ".xhtml", ".shtml", ".xml", ".svg", ".css", ".scss", ".sass",
    ".less", ".styl", ".jsx", ".haml", ".pug", ".jade", ".vue", ".svelte", ".astro", 
    ".hbs", ".handlebars", ".ejs", ".mjml", ".twig", ".liquid",
    
    // Documentation/Text
    ".md", ".markdown", ".txt", ".text", ".rtf", ".tex", ".latex", ".rst", ".asciidoc", 
    ".adoc", ".asc", ".creole", ".wiki", ".mediawiki", ".org", ".pod", ".rdoc", 
    ".textile", ".nfo",
    
    // Data formats
    ".json", ".jsonl", ".jsonc", ".json5", ".yaml", ".yml", ".toml", ".ini", ".cfg", 
    ".conf", ".properties", ".prop", ".csv", ".tsv", ".ssv", ".dsv", ".env",
    
    // Config files
    ".gitignore", ".gitattributes", ".editorconfig", ".prettierrc", ".eslintrc",
    ".stylelintrc", ".babelrc", ".npmrc", ".yarnrc", ".htaccess", ".dockerignore",
    ".gitmodules", ".flowconfig", ".browserslistrc", ".gcloudignore", ".npmignore",
    ".nvmrc", ".psqlrc", ".sql", ".sqliterc",
    
    // Shell scripts and configs
    ".sh", ".bash", ".zsh", ".fish", ".bash_profile", ".bashrc", ".profile",
    ".zshrc", ".zshenv", ".zprofile", ".zlogin", ".zlogout", 
    
    // Project files
    ".csproj", ".vbproj", ".fsproj", ".pyproj", ".vcxproj", ".pom", ".gradle", 
    ".sbt", ".cabal", ".cmake", ".make", ".gn", ".gyp", ".gypi",
    
    // Miscellaneous
    ".dot", ".gv", ".graphql", ".gql", ".proto", ".plist", ".sublime-project",
    ".sublime-workspace", ".code-workspace", ".log", ".diff", ".patch",
  ];

  // Combine default and custom extensions
  const allTextExtensions = [...textFileExtensions, ...customTextExtensions];

  // First check: Is it a known text file extension?
  if (extension && allTextExtensions.includes(extension)) {
    return true;
  }

  // Second check: Is it a known textual file without extension?
  const knownTextFilenames = [
    "makefile", "dockerfile", "license", "readme", "authors", "changelog",
    "news", "copyright", "contributing", "makefile", "gemfile", "rakefile",
    "jenkinsfile", ".gitignore", ".gitattributes", ".editorconfig", ".env",
    "vagrantfile", "procfile", "cargo.toml", "package.json", "tsconfig.json"
  ];

  const filename = path.split("/").pop()?.toLowerCase() || "";
  if (knownTextFilenames.includes(filename)) {
    return true;
  }

  // In strict mode, files without whitelisted extensions are rejected
  if (strictMode && extension) {
    return false;
  }
  
  // If we have content and fallback is enabled, perform content analysis for ambiguous files
  if (fallbackToContentCheck && content) {
    return isContentLikelyText(content);
  }

  // Default to false when uncertain and not in fallback mode
  return false;
}

/**
 * Helper function to analyze content for text characteristics
 * Only used when extension-based checks are inconclusive
 */
function isContentLikelyText(content: string): boolean {
  if (!content || content.length === 0) {
    return true; // Empty files are considered text
  }

  // Sample the content (for performance with large files)
  const sampleSize = Math.min(4096, content.length);
  const sample = content.substring(0, sampleSize);
  
  // Quick check: Any null bytes strongly indicate binary
  if (sample.includes('\0')) {
    return false;
  }
  
  // Check for high concentration of control characters (excluding whitespace)
  let controlCharCount = 0;
  let totalChars = 0;
  
  for (let i = 0; i < sample.length; i++) {
    totalChars++;
    const code = sample.charCodeAt(i);
    
    // Count control characters (except common whitespace)
    if ((code < 32 && ![9, 10, 13].includes(code)) || (code >= 127 && code < 160)) {
      controlCharCount++;
    }
  }
  
  // If more than 10% are control chars, probably binary
  return (controlCharCount / totalChars) < 0.1;
}
```

## Key Features

1. **Extensive Extension Whitelist**: Comprehensive list of extensions for:
   - Programming languages
   - Web development files
   - Documentation formats
   - Configuration files
   - Data formats
   - Shell scripts

2. **Known Text Filenames**: Recognizes common text files that often don't have extensions (e.g., `LICENSE`, `Dockerfile`)

3. **Customizable**:
   - Add your own extensions via `customTextExtensions`
   - Toggle strict mode that only accepts whitelisted extensions
   - Enable/disable content-based fallback checks

4. **Fallback Analysis**: When necessary, can analyze file content, but prioritizes the more reliable extension-based approach

## Usage in BinaryIgnoringFileSystem

You would use this helper in your file system class like:

```typescript
import { isKnownTextFile } from "./utils/file-helpers";

export class BinaryIgnoringFileSystem extends BaseFileSystem {
  async walkDirectory(directory: string): Promise<string[]> {
    const allFiles = await super.getFilesInDirectory(directory);
    
    // Filter to only known text files
    const textFiles = allFiles.filter((filePath) => {
      const content = this.readFile(filePath);
      return isKnownTextFile(
        filePath, 
        content, 
        { 
          fallbackToContentCheck: true,
          strictMode: this.isStrictModeEnabled(),
          customTextExtensions: this.getCustomTextExtensions() 
        }
      );
    });
    
    return textFiles;
  }
  
  // Configuration methods
  private isStrictModeEnabled(): boolean {
    // Return configuration or environment setting
    return process.env.STRICT_TEXT_MODE === "true";
  }
  
  private getCustomTextExtensions(): string[] {
    // Could come from config file or environment
    return (process.env.CUSTOM_TEXT_EXTENSIONS || "").split(",")
      .filter(ext => ext.trim().length > 0)
      .map(ext => ext.startsWith(".") ? ext : `.${ext}`);
  }
  
  // Testing helper
  isBinaryDetectionStrict(): boolean {
    return this.isStrictModeEnabled();
  }
}
```

This approach provides a balance between simplicity and accuracy. For most development workflows, a whitelist approach is more predictable than binary detection algorithms, which can sometimes produce false positives/negatives. The extensive list of known text extensions makes this particularly reliable in development environments.