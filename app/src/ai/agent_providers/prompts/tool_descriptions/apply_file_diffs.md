Batch-edit files. Each entry in `operations` is one of:

- `edit` — Replace one block of text in an existing file. Provide `file_path`, the existing text (`search`) and its replacement (`replace`); `old_string`/`new_string` are accepted as aliases. Each `edit` changes exactly one place. There is no `replace_all`: to change several occurrences, send one `edit` per occurrence, each with enough surrounding lines to be unique.
- `create` — Create a new file at the given path with the given content. Fails if the file already exists.
- `delete` — Delete the file at the given path.

# Edit guidelines

- Read the file with `read_files` first and copy `search` from its current content.
- `search` is matched on whole lines against the file as it is on disk. Copy complete lines, including their indentation. `replace` is written exactly as given and is not re-indented, so give it the indentation the file uses.
- If `search` matches more than one place, only the best (then the earliest) match is changed and the others are left as they are. When the text you want to change appears several times, include more surrounding lines so `search` is unique.
- An `edit` whose `search` is not found fails with the file unchanged; the result names the search block that did not match. Several edits to the same file are all matched against the original content, so do not make one edit depend on another edit's output.
- ALWAYS prefer editing existing files to creating new ones.
- NEVER proactively create documentation files (`*.md`, `README*`) unless the user explicitly asked.
- Only use emojis if the user explicitly requested it. Avoid emojis in code unless asked.

# Create guidelines

- This is the right tool for writing brand-new files. Do not use a shell `cat <<EOF` heredoc or `echo >` for this.
- Default to ASCII when creating files. Only introduce non-ASCII characters when there is a clear justification.

# Delete guidelines

- Be conservative — only delete files when the user explicitly asked or when it's an obvious cleanup of files you yourself just created and no longer need.

# Batching

You can pass multiple operations in a single call. They are checked together before anything is written: if any `search` cannot be matched, no file is changed. Files are then written one at a time; if a write fails part-way, the result says which files were updated and which were not, and those edits must not be sent again. Prefer batching when you are making related edits across files in the same task.
