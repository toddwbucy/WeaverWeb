// The browser's half of the passkey ceremonies (design section 4): it
// converts base64url to and from ArrayBuffer, fetches the server's options,
// calls the browser, and posts back exactly the members the server's library
// parses. No extension output, no device detail.

const fromB64 = (s) =>
  Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0)).buffer;

const toB64 = (buffer) =>
  btoa(String.fromCharCode(...new Uint8Array(buffer)))
    .replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

async function post(url, body) {
  const answer = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await answer.text();
  if (!answer.ok) throw new Error(text);
  return text ? JSON.parse(text) : null;
}

// The server's options at `url`/options, made ready for the browser.
async function options(url, body) {
  const { ceremony, options } = await post(url + "/options", body);
  const key = options.publicKey;
  key.challenge = fromB64(key.challenge);
  if (key.user) key.user.id = fromB64(key.user.id);
  for (const c of key.allowCredentials || key.excludeCredentials || []) c.id = fromB64(c.id);
  return [ceremony, { publicKey: key }];
}

// Registration: id, rawId, type, clientDataJSON, attestationObject, and
// transports where the browser gives them.
async function create(url, body) {
  const [ceremony, ask] = await options(url, body);
  const made = await navigator.credentials.create(ask);
  const r = made.response;
  const response = { clientDataJSON: toB64(r.clientDataJSON), attestationObject: toB64(r.attestationObject) };
  if (r.getTransports) response.transports = r.getTransports();
  return { ceremony, credential: { id: made.id, rawId: toB64(made.rawId), type: made.type, response } };
}

// Authentication: id, rawId, type, authenticatorData, clientDataJSON,
// signature and userHandle.
async function get(url, body) {
  const [ceremony, ask] = await options(url, body);
  const got = await navigator.credentials.get(ask);
  const r = got.response;
  const response = {
    authenticatorData: toB64(r.authenticatorData),
    clientDataJSON: toB64(r.clientDataJSON),
    signature: toB64(r.signature),
    userHandle: r.userHandle ? toB64(r.userHandle) : null,
  };
  return { ceremony, credential: { id: got.id, rawId: toB64(got.rawId), type: got.type, response } };
}

const enroll = async (token) => post("/enroll/finish", await create("/enroll", { token }));
const signIn = async (name) => post("/sign-in/finish", await get("/sign-in", { name }));
// Adding: a fresh assertion earns a one-time grant; a registration spends it.
async function add(label) {
  const { grant } = await post("/passkeys/add/assert/finish", await get("/passkeys/add/assert", {}));
  return post("/passkeys/add/finish", { ...(await create("/passkeys/add", { grant })), label });
}

// One form per page: the ceremony it runs, the field it reads, what it says.
function ceremony(id, field, run, done, failed) {
  const form = document.getElementById(id);
  if (!form) return;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const status = document.getElementById("status");
    status.textContent = "Waiting for your authenticator.";
    try {
      const answer = await run(form.elements[field].value.trim());
      form.reset();
      done(status, answer);
    } catch (error) {
      status.textContent = failed + error.message;
    }
  });
}

const go = (_, answer) => location.assign(answer.next);
ceremony("enroll", "token", enroll,
  (status) => { status.textContent = "Your passkey is enrolled. Sign in with it to continue."; },
  "Not enrolled: ");
ceremony("sign-in", "person", signIn, go, "Not signed in: ");
ceremony("add", "label", add, go, "Not added: ");
