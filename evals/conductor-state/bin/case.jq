# One feature row → the case: the delexicalized state line (the ONE writer, in
# lib.jq), the bind map, and the rule labels a hand label is measured against.
# Run as: jq -c -L bin -f bin/case.jq
include "lib";
. + { line: delex, bind: { "$a": .node }, rule: { condition: condition, decision: decision(condition), risk: risk(decision(condition)) } }
