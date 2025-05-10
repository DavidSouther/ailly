---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-05-10T09:11:32.335Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
isolated: true
prompt: |
  Write an implementation for `isAscii`, a helper used in BinaryIgnoringFileSystem,
  that takes a path as input and decides whether this file is binary or not based on its contents.
---

# `isAscii` Implementation for BinaryIgnoringFileSystem

Here's an implementation of `isAscii` that examines file contents to determine whether a file is ASCII (text) or binary. This method uses several heuristics common in binary detection algorithms:

```typescript
/**
 * Determines if a file contains text (ASCII/UTF-8) or binary data
 *
 * @param path The file path (used for potential extension checks)
 * @param content The file content as a string
 * @param options Optional configuration parameters
 * @returns boolean - true if the file appears to be text, false if it appears to be binary
 */
export function isAscii(
  path: string,
  content: string,
  options: {
    sampleSize?: number; // Number of bytes to examine (default: 512)
    nullsBinaryThreshold?: number; // Percentage of null bytes that indicates binary (default: 1%)
    controlCharThreshold?: number; // Percentage of control chars that indicates binary (default: 10%)
    knownBinaryExtensions?: string[]; // File extensions that are known to be binary
  } = {}
): boolean {
  // Configure options with defaults
  const {
    sampleSize = 512,
    nullsBinaryThreshold = 1, // 1% null bytes indicates binary
    controlCharThreshold = 10, // 10% control chars indicates binary
    // biome-ignore format: More a table than an array
    knownBinaryExtensions = [
      // Images
      ".png",
      ".jpg",
      ".jpeg",
      ".gif",
      ".bmp",
      ".ico",
      ".webp",
      ".tiff",
      // Audio/Video
      ".mp3",
      ".mp4",
      ".avi",
      ".mov",
      ".wav",
      ".flac",
      ".ogg",
      // Archives
      ".zip",
      ".tar",
      ".gz",
      ".rar",
      ".7z",
      ".bz2",
      // Documents
      ".pdf",
      ".doc",
      ".docx",
      ".xls",
      ".xlsx",
      ".ppt",
      ".pptx",
      // Executables
      ".exe",
      ".dll",
      ".so",
      ".dylib",
      ".bin",
      // Other binary formats
      ".ttf",
      ".woff",
      ".woff2",
      ".class",
      ".pyc",
      ".o",
    ],
  } = options;

  // 1. Check file extension for known binary types
  const extension = path.substring(path.lastIndexOf(".")).toLowerCase();
  if (knownBinaryExtensions.includes(extension)) {
    return false; // Known binary extension
  }

  if (hasBinarySignature(content)) {
    return false;
  }

  // Look for null bytes and control characters
  let nullCount = 0;
  let controlCount = 0;
  let sampleBytes = Math.min(sampleSize, content.length);

  for (let i = 0; i < sampleBytes; i++) {
    const charCode = contents.charCodeAt(i);

    // Check for null byte
    if (charCode === 0) {
      nullCount++;
    }

    // Check for non-printable control characters (except common whitespace)
    if (
      (charCode < 32 && ![9, 10, 13].includes(charCode)) ||
      (charCode >= 127 && charCode < 160)
    ) {
      controlCount++;
    }
  }

  // Calculate percentages of suspicious characters
  const nullPercentage = (nullCount / totalBytes) * 100;
  const controlPercentage = (controlCount / totalBytes) * 100;

  if (
    nullPercentage >= nullsBinaryThreshold ||
    controlPercentage >= controlCharThreshold
  ) {
    return false; // Likely binary
  }

  return true;
}

/**
 * Checks if the content starts with common binary file signatures (magic numbers)
 */
function hasBinarySignature(content: string): boolean {
  if (!content || content.length < 4) return false;

  // Extract the first few bytes for signature checking
  const bytes: number[] = [];
  for (let i = 0; i < Math.min(content.length, 8); i++) {
    bytes.push(content.charCodeAt(i));
  }

  // Convert bytes to hex for easier signature comparison
  const signature = bytesToHex(bytes.slice(0, 8));

  // Check against common file signatures (magic numbers)
  const signatures: Record<string, string> = {
    // Images
    PNG: "89504e470d0a1a0a", // PNG
    "JPEG/JPG": "ffd8ff", // JPEG/JPG (looking at first 3 bytes)
    GIF87a: "474946383761", // GIF87a
    GIF89a: "474946383961", // GIF89a
    BMP: "424d", // BMP
    WEBP: "52494646", // WEBP

    // Archives
    ZIP: "504b0304", // ZIP
    GZIP: "1f8b", // GZIP
    RAR: "526172", // RAR
    "7Z": "377abcaf271c", // 7Z

    // PDF
    PDF: "25504446", // PDF

    // Executables
    "EXE/DLL": "4d5a", // EXE/DLL (MZ)
    ELF: "7f454c46", // ELF

    // Office documents
    DOC: "d0cf11e0a1b11ae1", // Office documents
    DOCX: "504b0304", // Office XML (same as ZIP)
  };

  // Check if content starts with any known binary signature
  for (const [format, sig] of Object.entries(signatures)) {
    if (signature.startsWith(sig.toLowerCase())) {
      return true;
    }
  }

  return false;
}

/**
 * Convert an array of bytes to a lowercase hex string
 */
function bytesToHex(bytes: number[]): string {
  return bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}
```

## Key Features of this Implementation

1. **Multi-faceted Detection**: Uses several methods to detect binary files:

   - File extension checking
   - Binary signature (magic number) detection
   - Null byte percentage analysis
   - Control character frequency analysis

2. **Configurable Parameters**: Allows customization of detection thresholds and sample size

3. **Performance Optimization**: Only samples the beginning of large files rather than scanning the entire content

4. **Comprehensive Magic Number Detection**: Recognizes common binary file formats by their signatures

5. **Whitelist for Common Control Characters**: Allows common whitespace characters (tab, newline, carriage return) as they're normal in text files

## Usage in BinaryIgnoringFileSystem

You would use this helper in your `BinaryIgnoringFileSystem` class like this:

```typescript
import { isAscii } from "./utils/file-helpers";

export class BinaryIgnoringFileSystem extends BaseFileSystem {
  async walkDirectory(directory: string): Promise<string[]> {
    const allFiles = await super.getFilesInDirectory(directory);

    // Filter out binary files
    const textFiles = allFiles.filter((filePath) => {
      const content = this.readFile(filePath);
      return isAscii(filePath, content);
    });

    return textFiles;
  }

  // For testing purposes
  isBinaryDetectionStrict(): boolean {
    // Return whether we're using strict binary detection settings
    return true;
  }
}
```

This implementation balances accuracy with performance by using a combination of methods that are commonly used for binary file detection. The configurable parameters allow you to tune the algorithm according to your specific needs.
