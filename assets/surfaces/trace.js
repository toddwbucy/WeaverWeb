// The agent page's Live trace pane (Spec 2.13): it reads the listener's
// window as the stream sends it and places each event's own words in a row
// of the pane's list. It computes nothing the server did not send: the time
// is the trace record's `wall_ms` as it came, the classes say what the event
// says (`mark`, `replayed`), and a member an event does not carry is an
// `absent` pill, never a blank. The stream's URL is the list's `data-stream`.

const list = document.querySelector("ul.stream[data-stream]");

const node = (tag, className, text) => {
  const made = document.createElement(tag);
  if (className) made.className = className;
  if (text !== undefined) made.textContent = text;
  return made;
};

// A member as it came, or the absent pill where the event carries none.
const word = (value) =>
  value === null || value === undefined
    ? node("span", "absent", "absent")
    : document.createTextNode(typeof value === "string" ? value : JSON.stringify(value));

// One row: the sequence, the time, then the event's fields; a mark's reason
// in place of the fields. A row the listener judged the replay's says so.
function row(event) {
  const li = node("li", event.mark ? "mark" : "");
  if (event.replayed) li.classList.add("replayed");
  li.append(node("span", "seq", event.mark ? "" : String(event.seq)));
  const time = node("span");
  time.append(word(event.raw && typeof event.raw === "object" ? event.raw.wall_ms : undefined));
  const fields = node("span");
  if (event.mark) {
    fields.append(event.mark);
  } else {
    fields.append(word(event.kind), " run:", word(event.run), " turn:", word(event.turn));
  }
  if (event.replayed) fields.append(" ", node("span", "status hold", "replayed"));
  li.append(time, fields);
  return li;
}

if (list) {
  const pane = list.closest(".pane");
  const raw = pane.querySelector("pre.raw");
  const said = pane.querySelector("footer .window");
  const state = pane.querySelector("footer .state");
  const stream = new EventSource(list.dataset.stream);
  // An event and a mark are JSON; the server's own lag mark is a sentence.
  // The pane bounds the list; a reader at its end is kept at the newest row,
  // a reader scrolled back is left where they are. Only the list scrolls,
  // never the page, and no row is dropped: the list holds what the window
  // sends.
  const take = (message) => {
    let event;
    try {
      event = JSON.parse(message.data);
    } catch {
      event = { mark: message.data };
    }
    const atEnd = list.scrollTop + list.clientHeight >= list.scrollHeight - 1;
    list.append(row(event));
    if (atEnd) list.scrollTop = list.scrollHeight;
    raw.textContent = message.data;
  };
  stream.addEventListener("trace", take);
  stream.addEventListener("mark", take);
  // The window's word opens every connection, and the stream sends the
  // whole window after it. The browser reopens a dropped stream on its own,
  // so a connection's start clears what the last one placed: the pane shows
  // the window once, as this connection sends it.
  const unread = raw.textContent;
  stream.addEventListener("window", (message) => {
    list.replaceChildren();
    raw.textContent = unread;
    said.textContent = message.data;
  });
  // The view's end, with its reason: the session or the grant ended. The
  // stream is closed here, or the browser would open it again.
  stream.addEventListener("closed", (message) => {
    stream.close();
    state.textContent = message.data;
  });
}
