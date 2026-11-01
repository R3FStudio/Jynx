# Jynx assets

`branding/` contains the shared Jynx mark, size variants, application icon formats, and color values. Use `jynx-mark.svg` where a scalable transparent mark is needed. Use a PNG rendition where SVG is unsupported.

Keep the mark's proportions and transparent padding intact. The violet and teal colors belong to the same palette; avoid recoloring individual parts of the mark. Artwork usage terms are in [branding/LICENSE.txt](branding/LICENSE.txt).

`showcase/` contains the CLI presentation used by the project README. It presents captured Jynx CLI output in a styled terminal frame, using a controlled local provider and real execution of a Rust flight telemetry project's tests. It is not a terminal-app screenshot or a model-speed benchmark.
