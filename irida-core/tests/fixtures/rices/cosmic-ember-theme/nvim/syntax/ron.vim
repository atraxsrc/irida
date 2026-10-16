" Minimal regex syntax for RON (Rusty Object Notation), used when no
" treesitter parser is installed. Shipped with cosmic-ember-theme.
if exists("b:current_syntax") | finish | endif

syn keyword ronBoolean true false
syn keyword ronOption Some None
syn match   ronNumber  /\<-\=\d\+\(\.\d\+\)\=\>/
syn region  ronString  start=/"/ skip=/\\"/ end=/"/
syn match   ronComment "//.*$"
syn region  ronComment start="/\*" end="\*/"
syn match   ronField   /\<[a-z_][A-Za-z0-9_]*\ze\s*:/
syn match   ronEnum    /\<[A-Z][A-Za-z0-9_]*\>/
syn match   ronPunct   /[(){}\[\],:]/

hi def link ronBoolean Boolean
hi def link ronOption  Keyword
hi def link ronNumber  Number
hi def link ronString  String
hi def link ronComment Comment
hi def link ronField   Identifier
hi def link ronEnum    Type
hi def link ronPunct   Delimiter

let b:current_syntax = "ron"
