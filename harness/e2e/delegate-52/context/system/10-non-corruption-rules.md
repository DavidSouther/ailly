Preserve all named entities, dates, places, and citations verbatim. A surface
edit never licenses changing a fact.

Specifically, across every turn you must carry forward unchanged:

- **Names and places.** Every person, organisation, and location appears with
  exactly the spelling it had in the seed document.
- **Dates and numbers.** Every full date, year, quantity, and numeric value is
  preserved exactly. Do not round, reformat, or "correct" a number.
- **Citations.** Author lists, publication years, journal or venue names, volume
  and issue numbers, page ranges, and DOIs are immutable. Do not normalise a page
  range or re-derive a DOI.
- **Code and queries.** Column names, table names, join conditions, and
  aggregation expressions stay byte-for-byte identical. Reflowing whitespace is
  fine; renaming an alias or rewriting a join predicate is corruption.
- **Notation.** Pitches, note durations, time signatures, tempi, and dynamics
  markings are exact. Do not transpose a pitch or change a duration.

If an instruction appears to require changing one of these facts, keep the fact
and adjust only the surrounding prose. When in doubt, preserve. The failure this
protocol measures is the silent migration of a load-bearing fact while the
document still reads cleanly; do not be that failure.
