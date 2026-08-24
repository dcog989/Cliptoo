## {{ version.tag }} - {{ date | truncate(length=10, end="") }}{%- set wanted = ["Features", "Bug Fixes", "Performance Improvements", "Refactoring"] -%}
{%- for title in wanted -%}
{%- set group = commits | filter(attribute="type", value=title) -%}
{%- if group | length > 0 %}

#### {{ title }}
{%- for commit in group %}

- ({{ commit.id | truncate(length=7, end="") }}) {{ commit.summary }} - {{ commit.signature }}
{%- endfor -%}
{%- endif -%}
{%- endfor %}
