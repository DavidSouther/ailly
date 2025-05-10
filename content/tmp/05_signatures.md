---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-05-10T10:04:21.734Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
isolated: true
prompt: |
  Given this list of known binary file format prefixes.

  ```
  // biome-ignore format:
  const SIGNATURES = [
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
  ```

  Generate more signatures for this list.
---

# Additional Binary File Format Signatures

Here are more signatures you can add to your existing list:

```javascript
// Documents
"%PDF-", // 'pdf' (more precise than just '%PDF')
  "{\\rtf", // 'rtf' - Rich Text Format
  "PK\x03\x04\x14\x00\x06\x00", // 'docx/xlsx/pptx' - Office Open XML
  "\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1", // 'doc/xls/ppt' - Microsoft Compound File Binary Format
  // Audio files
  "ID3", // 'mp3' with ID3 tag
  "RIFF....WAVE", // 'wav' (where .... is 4 bytes for file size)
  "\xFF\xFB", // 'mp3' without ID3
  "OggS", // 'ogg'
  "fLaC", // 'flac'
  // Video files
  "\x00\x00\x00\x18\x66\x74\x79\x70\x6D\x70\x34\x32", // 'mp4'
  "\x00\x00\x00\x1C\x66\x74\x79\x70\x4D\x34\x56\x20", // 'm4v'
  "RIFF....AVI ", // 'avi' (.... is file size)
  "\x1A\x45\xDF\xA3", // 'mkv/webm' - Matroska/WebM
  // Fonts
  "OTTO", // 'otf' - OpenType font
  "true", // 'ttf' - TrueType font
  "wOFF", // 'woff' - Web Open Font Format
  // Other formats
  "\x25\x21\x50\x53", // 'ps' - PostScript
  "\x89\x50\x4E\x47\x0D\x0A\x1A\x0A", // 'png' (more complete signature)
  "BPG\xFB", // 'bpg' - Better Portable Graphics
  "8BPS", // 'psd' - Photoshop Document
  "II*\x00", // 'tiff' - little endian
  "MM\x00*", // 'tiff' - big endian
  "FWS", // 'swf' - Flash (uncompressed)
  "CWS", // 'swf' - Flash (compressed)
  "FLV\x01", // 'flv' - Flash Video
  // Database files
  "SQLite format 3\x00", // 'sqlite'
  "\x53\x51\x4C\x69\x74\x65\x20\x66\x6F\x72\x6D\x61\x74\x20\x33\x00"; // 'sqlite' (in hex)
```

These signatures cover a wider range of common file formats and should help make your file type detection more robust. Note that some formats (particularly container formats) might require more sophisticated detection beyond just examining the first few bytes.
