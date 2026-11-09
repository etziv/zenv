# Features that use an LLM

Use when the project calls a model (OpenAI, Anthropic, Gemini SDKs, Prism, a hosted chatbot), retrieves documents
for it (RAG), stores memory for it, or lets it call tools or MCP servers.

Discipline: text that tries to steer the model is not a finding on its own. A finding needs a failure in our code:
attacker-written content reaches another user's model context, the model gets authority the requesting user does not
have, it leaks data the user may not read, or its output reaches a sink (HTML, SQL, a URL fetch, a payment) without
the checks that sink needs. A system-prompt instruction ("never reveal...") is not a control; only code is: scoped
queries, authorization in tool handlers, allowlists, confirmation bound to the exact action, budgets.

## Context and retrieval
- **Indirect injection.** Who can write what the model reads: product reviews, uploaded documents, tickets, emails,
  web pages fetched for it, tool results. Then: whose session reads it, and which tools that session has. Text from
  user A reaching an admin's assistant that can change data is the main case.
- **Retrieval scope.** The vector or keyword search filters by tenant and permissions inside the query, and cached
  answers or embeddings are keyed per tenant.
- **Roles from the client.** A frontend that sends the whole `messages` array, so the user can add a `system` message
  or fake earlier assistant or tool turns. The server should own the system prompt and history.
- **Stored memory.** Summaries or "facts" the model writes that are later read in another user's session, or as rules
  in a more privileged one.

## Tools and actions
- **Who acts.** Tool handlers run with the requesting user's identity and re-check that user's permission on the named
  record, not with a service account that can do everything.
- **Arguments are untrusted input.** Model-produced arguments go through the same validation as a form: SQL, file
  paths, URLs (SSRF), ids of other users' records. A JSON schema checks shape, not permission.
- **Confirmation.** When the user approves an action ("refund Rp 50.000 to order 123"), the executed call must use
  exactly the approved arguments; a later model turn or a retry must not change them or run them twice.
- **Loops and spend.** Per-request limits on tool calls, tokens and side effects (mails, SMS, payments); a per-user
  quota on paid model calls.

## Output and secrets
- Model output rendered as HTML or Markdown with raw HTML is a stored or reflected XSS sink (`client-side.md`);
  Markdown images to attacker URLs can carry data out in the query string.
- The prompt contains only what this user may see: no API keys, other users' rows or internal config that the user
  could get the model to repeat.
- Provider keys stay on the server: a `VITE_OPENAI_*` style variable ships the key to every visitor.

## Agents, sub-agents, MCP
Delegated calls get the least authority they need, and what they hand back is checked like any other input. Tool and
server names, descriptions and schemas from an MCP server guide the model but grant nothing: the allowlist and the
handler's authorization decide. Two servers offering the same tool name must not be confusable.
