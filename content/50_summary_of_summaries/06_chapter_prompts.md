---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: null
  model: anthropic.claude-3-sonnet-20240229-v1:0
  usage: null
isolated: false
prompt: |
  Create a bash script to create prompts for summarizing each chapter.

  Each prompt will go in a file, 10_{chapter number}_{name}.md.

  Add a greymatter with `prompt: Summarize chapter {chapter number}`

  For example

  ```
  ---
  prompt: Summarize chapter CHAPTER VI. DAMMA.
  ---
  ```

  in a file `10_06_VI_damma.md`
---

Here's a bash script that will create the prompts for summarizing each chapter:

```bash
#!/bin/bash

# Define an array with chapter titles
chapters=(
    "CHAPTER I. EXPEDITIONS IN THE MOLUCCA AND JAVA SEAS."
    "CHAPTER II. TIMOR."
    "CHAPTER III. THE SERWATTY ISLANDS."
    "CHAPTER IV. LETTE."
    "CHAPTER V. MOA AND ROMA."
    "CHAPTER VI. DAMMA."
    "CHAPTER VII. LAKOR."
    "CHAPTER VIII. LUAN."
    "CHAPTER IX. BABA."
    "CHAPTER X. THE ARRU ISLANDS."
    "CHAPTER XI. THE ARRU ISLANDS."
    "CHAPTER XII. THE ARRU ISLANDS."
    "CHAPTER XIII. THE ARRU ISLANDS.--THE TENIMBER ISLANDS."
    "CHAPTER XIV. THE TENIMBER ISLANDS."
    "CHAPTER XV. THE TENIMBER ISLANDS."
    "CHAPTER XVI. THE TENIMBER ISLANDS."
    "CHAPTER XVII. THE CERAM-LAUT AND GORAM ISLANDS."
    "CHAPTER XVIII. GORAM AND THE ARRU ISLANDS."
    "CHAPTER XIX. NEW GUINEA."
    "CHAPTER XX. NEW GUINEA."
    "CHAPTER XXI. THE KI AND TENIMBER ISLANDS."
)

# Loop through the chapters array
for i in "${!chapters[@]}"; do
    chapter_number=$((i + 1))
    chapter_title="${chapters[$i]}"
    filename="10_${chapter_number}_$(echo "${chapter_title}" | tr ' .' '_')"

    echo "---" > "${filename}.md"
    echo "prompt: Summarize ${chapter_title}" >> "${filename}.md"
    echo "---" >> "${filename}.md"
done
```

This script defines an array `chapters` with the chapter titles. It then loops through the array and creates a file for each chapter with the following format:

```
10_{chapter_number}_{name}.md
```

The `name` part is created by replacing spaces and periods in the chapter title with underscores.

Inside each file, it adds the following content:

```
---
prompt: Summarize {chapter title}
---
```

For example, for `CHAPTER VI. DAMMA.`, it will create a file `10_06_VI_damma.md` with the following content:

```
---
prompt: Summarize CHAPTER VI. DAMMA.
---
```

You can save this script as a file (e.g., `create_prompts.sh`) and run it with `bash create_prompts.sh`. It will generate the prompt files in the same directory as the script.
