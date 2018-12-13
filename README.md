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
  + (edit) edit command
  + (Enter) execute command
  + (C-C) return to display mode
- commands:
  + see `file`
  + end

filter modes

- allow user to select desired text to act upon
- inputs:
  + (edit) edit filter input
  + (C-C) return to display mode
- filters:
  + line number
    * (digits) line number

action mode

mark mode

- allow user to edit view
- inputs:
  + (edit) edit view
