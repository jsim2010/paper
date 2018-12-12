# paper

## terms

view = the entire file
window = the currently visible portion of the view

## process

display mode

- display view through window
- inputs:
  + (.) change to command mode
  + (/) change to filter mode
  + (j) move window 1/4 window down
  + (k) move window 1/4 window up
  + move window

command mode

- allow user to enter commands involving views
- display command being entered along with suggestions
- inputs:
  + (edit) edit command
  + (Enter) execute command
- commands:
  + see `file`
  + end

filter mode

- allow user to select desired text to act upon
- inputs:
  + (l) line
    * (digits) line number

action mode

mark mode

- allow user to edit view
- inputs:
  + (edit) edit view
