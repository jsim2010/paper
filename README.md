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
  + (/) filter by search item
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
  + put
  + end

filter modes

- allow user to select desired text to act upon
- inputs:
  + (Enter) change to action mode
  + (Tab) stack a new filter
  + (edit) edit filter input
  + (C-C) return to display mode

(#) Line number
- (digits) line number
- (digits)(.)(digits) range between 2 numbers
- (digits)(+/-)(digits) range starting at first number and going 2nd number up or down

action mode

- user specifies an action to be taken on the filtered text
- inputs:
  + (i) insert before filter
  + (I) insert after filter

edit mode

- allow user to edit view
- inputs:
  + (edit) edit view
