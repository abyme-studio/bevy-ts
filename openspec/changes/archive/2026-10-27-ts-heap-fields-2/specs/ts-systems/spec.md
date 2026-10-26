## ADDED Requirements

### Requirement: Message writers and readers carry heap fields
A message with a heap field SHALL be written by `MessageWriter.write(M.of({...}))`, `commands.write` and `MessageWriter.next()`, whose view's heap fields are assigned whole (`m.text = "hi"`) and sent with the record; a heap field left unset SHALL take its default. A reader SHALL read a message's heap field on demand, at most one host call per field per message for the record's life. A message type with no heap field SHALL keep today's queue and make no host call.

#### Scenario: Writing through next
- **WHEN** a system calls `const m = writer.next(); m.text = "go"; m.amount = 2` and a later reader reads `Chat`
- **THEN** it reads `text` `"go"` and `amount` 2

#### Scenario: A plain message is unchanged
- **WHEN** a system reads 1,000 `Damage` messages whose fields are all plain
- **THEN** it makes no host call for them
