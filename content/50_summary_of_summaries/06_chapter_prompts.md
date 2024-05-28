---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: msg_01GtysaS3DnQcsounM2B3nEk
  lastRun: 2024-05-28T01:48:58.966Z
  model: claude-3-haiku-200k-20240307
  statistics:
    firstByteLatency: 8637
    inputTokenCount: 112733
    invocationLatency: 11356
    outputTokenCount: 269
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

Here's a bash script that will create the prompts for each chapter and save them in individual files:

```bash
#!/bin/bash

for i in {1..21}
do
    chapter_num=$(printf "%02d" $i)
    chapter_name=$(grep -A 1 "CHAPTER $chapter_num" voyages.txt | tail -n 1 | sed 's/\./\n/g' | awk -F'.' '{print $2}' | sed 's/^ //')
    file_name="10_${chapter_num}_${chapter_name}.md"
    prompt="---
prompt: Summarize chapter CHAPTER $chapter_num. $chapter_name.
---"
    echo "$prompt" > $file_name
done
```

This script assumes that the book text is in a file called `voyages.txt`. It will loop through chapters 1 to 21, extract the chapter number and name, and then create a file with the prompt in the desired format.

The resulting files will be named `10_01_I_expeditions.md`, `10_02_II_timor.md`, and so on.