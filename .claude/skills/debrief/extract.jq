def text_of:
  if type == "string" then .
  else ([.[]? | select(.type == "text") | .text] | join("\n"))
  end;

def clip(n): if length > n then .[0:n] + " [...]" else . end;

def machine_text:
  startswith("<")
  or startswith("Another Claude session sent a message")
  or startswith("Base directory for this skill")
  or startswith("[SYSTEM NOTIFICATION")
  or startswith("This session is being continued");

def answer_lines:
  . as $r
  | [ $r.questions[]?
      | . as $q
      | ([$q.options[]?.label | select(test("\\(Recommended\\)"))] | first // "none") as $rec
      | ($r.answers[$q.question] // "(no answer)") as $chosen
      | ($r.annotations[$q.question].notes // "") as $notes
      | "  Q[\($q.header)]: \($q.question)\n    recommended: \($rec)\n    chosen: \($chosen)"
        + (if $chosen == $rec then "  (took the recommendation)" else "  (OVERRODE)" end)
        + (if $notes != "" then "\n    notes: \($notes)" else "" end)
    ]
  | join("\n");

def events:
  if .type == "assistant" then
    {before: (.message.content | text_of)}
  elif .type == "attachment" and .attachment.type == "queued_command" and .attachment.origin.kind == "human" then
    {kind: "mid-turn message", text: (.attachment.prompt | tostring)}
  elif .type == "user" and (.toolUseResult | type) == "object" and (.toolUseResult.answers? != null) then
    {kind: "answer to a question", text: (.toolUseResult | answer_lines)}
  elif .type == "user" and (.message.content | type) == "array"
       and ([.message.content[] | select(.type == "tool_result" and .is_error == true) | .content | text_of | select(startswith("The user doesn't want to proceed"))] | length > 0) then
    {kind: "rejected tool call", text: ([.message.content[] | select(.type == "tool_result" and .is_error == true) | .content | text_of] | join("\n"))}
  elif .type == "user" and (.isMeta | not) then
    (.message.content | text_of) as $t
    | if ($t | test("^\\[Request interrupted by user")) then {kind: "interrupt", text: $t}
      elif ($t | length) > 0 and ($t | machine_text | not) then {kind: "prompt", text: $t}
      else empty end
  else empty end;

foreach (.[] | events) as $e (
  {before: "", n: 0, seen: {}, out: null};
  if $e.before != null then
    (if ($e.before | length) > 0 then .before = $e.before else . end) | .out = null
  elif .seen[$e.text] then .out = null
  else
    .n += 1
    | .seen[$e.text] = true
    | .out = "#\(.n) \($e.kind)\n  you had just said: \(.before | gsub("\n"; " ") | clip(400))\n  user: \($e.text | clip(2000))\n"
  end;
  .out // empty
)
