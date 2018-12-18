# paper

## terms

view = the entire file
window = the currently visible portion of the view

## process

display mode

- display view through window
- inputs:
  + (.) change to command mode
  + (#) filter by line number
  + (j) move window 1/4 window down
  + (k) move window 1/4 window up

command mode

- allow user to enter commands involving views
- display command being entered along with suggestions
- inputs:
  + (Enter) execute command
  + (edit) edit command
  + (C-C) return to display mode
- commands:
  + see `file`
  + end

filter modes

- allow user to select desired text to act upon
- inputs:
  + (Enter) change to action mode
  + (Tab) join a new filter
  + (edit) edit filter input
  + (C-C) return to display mode
- filters:
  + line number
    * `#`: line number
    * `filter`|`filter`: 2 separate line number filters
    * `#``motion``#`: start at line number, include motion

action mode

- user specifies an action to be taken on the filtered text
- inputs:
  + (i) insert before filter
  + (I) insert after filter

edit mode

- allow user to edit view
- inputs:
  + (edit) edit view
