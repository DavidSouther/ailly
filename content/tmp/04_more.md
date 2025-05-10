---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-05-10T10:02:16.586Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
isolated: true
prompt: |
  Given these lists of constants
  ```
  // biome-ignore format:
  const TEXT_EXTENSIONS = new Set([
  // Programming Languages
  ".bash", ".c", ".clj", ".cljc", ".cljs", ".coffee", ".cpp", ".cs", ".d",
  ".dart", ".edn", ".elm", ".erl", ".ex", ".exs", ".f", ".f90", ".fs", ".fsx",
  ".go", ".groovy", ".hs", ".java", ".jl", ".js", ".jsx", ".kt", ".kts",
  ".lisp", ".lua", ".m", ".ml", ".mm", ".nim", ".pas", ".php", ".pl", ".pm",
  ".pp", ".py", ".r", ".rb", ".re", ".rs", ".sc", ".scala", ".sh", ".sql",
  ".swift", ".ts", ".tsx", ".vb", ".zsh",

  // Markup/Web
  ".astro", ".css", ".ejs", ".haml", ".handlebars", ".hbs", ".htm", ".html",
  ".jade", ".jsx", ".less", ".liquid", ".mjml", ".pug", ".sass", ".scss",
  ".shtml", ".styl", ".svelte", ".svg", ".twig", ".vue", ".xhtml", ".xml",

  // Documentation/Text
  ".adoc", ".asc", ".asciidoc", ".creole", ".latex", ".markdown", ".md",
  ".mediawiki", ".nfo", ".org", ".pod", ".rdoc", ".rst", ".rtf", ".tex",
  ".text", ".textile", ".txt", ".wiki",

  // Data formats
  ".cfg", ".conf", ".csv", ".dsv", ".env", ".ini", ".json", ".json5", ".jsonc",
  ".jsonl", ".prop", ".properties", ".ssv", ".toml", ".tsv", ".yaml", ".yml",

  // Config files
  // ".babelrc", ".browserslistrc", ".dockerignore", ".editorconfig", ".eslintrc",
  // ".flowconfig", ".gcloudignore", ".gitattributes", ".gitignore", ".gitmodules",
  // ".htaccess", ".npmignore", ".npmrc", ".nvmrc", ".prettierrc", ".psqlrc",
  // ".sqliterc", ".stylelintrc", ".yarnrc",

  // Shell scripts and configs
  ".bash", ".bash_profile", ".bashrc", ".fish", ".profile", ".sh", ".zlogin",
  ".zlogout", ".zprofile", ".zsh", ".zshenv", ".zshrc",

  // Project files
  ".cabal", ".cmake", ".csproj", ".fsproj", ".gn", ".gradle", ".gyp", ".gypi",
  ".make", ".pom", ".pyproj", ".sbt", ".vbproj", ".vcxproj",

  // Miscellaneous
  ".dot", ".gv", ".graphql", ".gql", ".proto", ".plist",
  ]);

  // biome-ignore format:
  const BINARY_EXTENSIONS = new Set([
  ".7z", ".a", ".bin", ".bmp", ".class", ".dat", ".db", ".dll", ".doc", ".docx",
  ".dylib", ".ear", ".exe", ".gif", ".gz", ".ico", ".jar", ".jpeg", ".jpg",
  ".lib", ".o", ".obj", ".pdf", ".png", ".ppt", ".pptx", ".pyc", ".pyo", ".rar",
  ".so", ".sqlite", ".tar", ".tif", ".tiff", ".war", ".xls", ".xlsx", ".zip",
  ]);
  ```

  rewrite BINARY_EXTENSIONS to look like TEXT_EXTENSIONS.
---
I'll rewrite the BINARY_EXTENSIONS constant to follow the same organizational style and formatting as TEXT_EXTENSIONS:

```javascript
// biome-ignore format:
const BINARY_EXTENSIONS = new Set([
// Archives/Compressed
".7z", ".ear", ".gz", ".rar", ".tar", ".war", ".zip",

// Executables/Libraries
".a", ".bin", ".class", ".dat", ".dll", ".dylib", ".exe", ".jar", ".lib",
".o", ".obj", ".pyc", ".pyo", ".so",

// Images
".bmp", ".gif", ".ico", ".jpeg", ".jpg", ".png", ".tif", ".tiff",

// Documents/Office files
".doc", ".docx", ".pdf", ".ppt", ".pptx", ".xls", ".xlsx",

// Database/Data files
".db", ".sqlite",
]);
```

I've organized the binary extensions into logical categories similar to how TEXT_EXTENSIONS is structured, with headers for each group and similar formatting with multiple extensions per line.