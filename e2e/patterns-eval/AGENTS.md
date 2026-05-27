# Patterns skill eval

This project's constitution. Both assemblies name this file explicitly at
position zero of the prefix so it is always the first thing the model
reads.

This project is a regression harness for the `patterns:*` plugin from
[davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design).
Two axes are exercised: **discovery** asks the model to pick the right
skill from its `description:` frontmatter alone, and **invocation** asks
the model to produce code that structurally exhibits the named pattern
once the skill is loaded.
