use std::collections::HashSet;
use std::sync::LazyLock;

pub const IGNORED_NAMES: &[&str] = &[".git", ".gitignore", ".vscode", ".idea"];

pub static TEXT_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Programming Languages
        ".bash",
        ".c",
        ".clj",
        ".cljc",
        ".cljs",
        ".coffee",
        ".cpp",
        ".cs",
        ".d",
        ".dart",
        ".edn",
        ".elm",
        ".erl",
        ".ex",
        ".exs",
        ".f",
        ".f90",
        ".fs",
        ".fsx",
        ".go",
        ".groovy",
        ".hs",
        ".java",
        ".jl",
        ".js",
        ".jsx",
        ".kt",
        ".kts",
        ".lisp",
        ".lua",
        ".m",
        ".ml",
        ".mm",
        ".nim",
        ".pas",
        ".php",
        ".pl",
        ".pm",
        ".pp",
        ".py",
        ".r",
        ".rb",
        ".re",
        ".rs",
        ".sc",
        ".scala",
        ".sh",
        ".sql",
        ".swift",
        ".ts",
        ".tsx",
        ".vb",
        ".zsh",
        // Markup/Web
        ".astro",
        ".css",
        ".ejs",
        ".haml",
        ".handlebars",
        ".hbs",
        ".htm",
        ".html",
        ".jade",
        ".less",
        ".liquid",
        ".mjml",
        ".pug",
        ".sass",
        ".scss",
        ".shtml",
        ".styl",
        ".svelte",
        ".svg",
        ".twig",
        ".vue",
        ".xhtml",
        ".xml",
        // Documentation/Text
        ".adoc",
        ".asc",
        ".asciidoc",
        ".creole",
        ".latex",
        ".markdown",
        ".md",
        ".mediawiki",
        ".nfo",
        ".org",
        ".pod",
        ".rdoc",
        ".rst",
        ".rtf",
        ".tex",
        ".text",
        ".textile",
        ".txt",
        ".wiki",
        // Data formats
        ".cfg",
        ".conf",
        ".csv",
        ".dsv",
        ".env",
        ".ini",
        ".json",
        ".json5",
        ".jsonc",
        ".jsonl",
        ".prop",
        ".properties",
        ".ssv",
        ".toml",
        ".tsv",
        ".yaml",
        ".yml",
        // Shell scripts and configs
        ".bash_profile",
        ".bashrc",
        ".fish",
        ".profile",
        ".zlogin",
        ".zlogout",
        ".zprofile",
        ".zshenv",
        ".zshrc",
        // Project files
        ".cabal",
        ".cmake",
        ".csproj",
        ".fsproj",
        ".gn",
        ".gradle",
        ".gyp",
        ".gypi",
        ".make",
        ".pom",
        ".pyproj",
        ".sbt",
        ".vbproj",
        ".vcxproj",
        // Miscellaneous
        ".dot",
        ".gv",
        ".graphql",
        ".gql",
        ".proto",
        ".plist",
    ]
    .into_iter()
    .collect()
});

pub static BINARY_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Archives/Compressed
        ".7z",
        ".ear",
        ".gz",
        ".rar",
        ".tar",
        ".war",
        ".zip",
        // Executables/Libraries
        ".a",
        ".bin",
        ".class",
        ".dat",
        ".dll",
        ".dylib",
        ".exe",
        ".jar",
        ".lib",
        ".o",
        ".obj",
        ".pyc",
        ".pyo",
        ".so",
        // Images
        ".bmp",
        ".gif",
        ".ico",
        ".jpeg",
        ".jpg",
        ".png",
        ".tif",
        ".tiff",
        // Documents/Office files
        ".doc",
        ".docx",
        ".pdf",
        ".ppt",
        ".pptx",
        ".xls",
        ".xlsx",
        // Database/Data files
        ".db",
        ".sqlite",
        // OpenType Fonts
        ".otf",
        ".ttf",
        ".ttc",
        // Web Fonts
        ".woff",
        ".woff2",
        ".eot",
        // PostScript Fonts
        ".pfb",
        ".pfm",
        ".ps",
        ".afm",
        ".cff",
        // Bitmap Fonts
        ".bdf",
        ".pcf",
        ".fon",
        ".fnt",
        // SVG Fonts
        ".svgz",
        // Font Collections/Utilities
        ".dfont",
        ".suit",
        ".suitcase",
        ".compositefont",
        ".oft",
        // Less Common Formats
        ".pfa",
        ".t42",
        ".gdr",
        ".abf",
        ".mxf",
        ".vlw",
        ".txf",
    ]
    .into_iter()
    .collect()
});

pub static SIGNATURES: &[&[u8]] = &[
    // Images
    b"\xFF\xD8\xFF",      // jpg/jpeg
    b"\x89PNG\r\n\x1A\n", // png
    b"GIF8",              // gif
    b"BM",                // bmp
    // Archives
    b"PK\x03\x04",         // zip/jar/apk
    b"\x1F\x8B\x08",       // gzip
    b"Rar!\x1A\x07",       // rar
    b"7z\xBC\xAF\x27\x1C", // 7z
    // Executables and binaries
    b"MZ",               // exe/dll
    b"\x7FELF",          // elf
    b"\xCA\xFE\xBA\xBE", // class/jar
    // PDF
    b"%PDF", // pdf
];
