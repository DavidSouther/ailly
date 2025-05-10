// Never include these files or directories
export const IGNORED_NAMES = [".git", ".gitignore", ".vscode", ".idea"];

// biome-ignore format:
export const TEXT_EXTENSIONS = new Set([
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
export const BINARY_EXTENSIONS = new Set([
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

  // OpenType Fonts
  ".otf", // OpenType Font
  ".ttf", // TrueType Font
  ".ttc", // TrueType Collection
  
  // Web Fonts
  ".woff", // Web Open Font Format
  ".woff2", // Web Open Font Format 2.0
  ".eot", // Embedded OpenType
    
  // PostScript Fonts
  ".pfb", // PostScript Font Binary
  ".pfm", // PostScript Font Metrics
  ".ps", // PostScript
  ".afm", // Adobe Font Metrics
  ".cff", // Compact Font Format

  // Bitmap Fonts
  ".bdf", // Bitmap Distribution Format
  ".pcf", // Portable Compiled Format
  ".fon", // Windows Font
  ".fnt", // Windows Font Resource

  // SVG Fonts
  ".svg", // Scalable Vector Graphics Font
  ".svgz", // Compressed SVG Font

  // Font Collections/Utilities
  ".dfont", // Mac OS X Data Fork Font
  ".suit", // Mac OS Font Suitcase Format
  ".suitcase", // Mac OS Font Suitcase
  ".compositefont", // macOS Composite Font
  ".bin", // Generic Binary Font File
  ".oft", // Older OpenType Format

  // Less Common Formats
  ".pfa", // PostScript Font ASCII
  ".t42", // Type 42 Font
  ".gdr", // GDI+ Font
  ".abf", // Adobe Binary Screen Font
  ".mxf", // Monotype Font Extensions
  ".vlw", // Processing Font Format
  ".txf", // Texture Font Format
]);

// biome-ignore format:
export const SIGNATURES = [
  // Images
  '\xFF\xD8\xFF', // 'jpg/jpeg',
  '\x89PNG\r\n\x1A\n', // 'png',
  'GIF8', // 'gif',
  'BM', // 'bmp',
  // Archives
  'PK\x03\x04', // 'zip/jar/apk',
  '\x1F\x8B\x08', // 'gzip',
  'Rar!\x1A\x07', // 'rar',
  '7z\xBC\xAF\x27\x1C', // '7z',
  // Executables and binaries
  'MZ', // 'exe/dll',
  '\x7FELF', // 'elf',
  '\xCA\xFE\xBA\xBE', // 'class/jar',
  // PDF
  '%PDF', // 'pdf',
];
