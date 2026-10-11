# Fuzz regression inputs

Each file here is an input that once crashed (or hung) a fuzz target. The
`fuzz_regressions` test feeds every file to the reader, the tokenizer and the
text decoders and checks that nothing panics. Name files
`<target>-<short-description>` and keep them as found (do not minimise by
hand). `tools/fuzz.sh` also seeds the fuzz corpora from this folder.
