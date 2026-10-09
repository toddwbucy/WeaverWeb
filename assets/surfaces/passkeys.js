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

// Registration, for a token's redemption: the credential's id, rawId, type,
// clientDataJSON and attestationObject, and its transports where the browser
// gives them.
async function enroll(token) {
  const { ceremony, options } = await post("/enroll/options", { token });
  const key = options.publicKey;
  key.challenge = fromB64(key.challenge);
  key.user.id = fromB64(key.user.id);
  for (const held of key.excludeCredentials || []) held.id = fromB64(held.id);
  const made = await navigator.credentials.create({ publicKey: key });
  const response = {
    clientDataJSON: toB64(made.response.clientDataJSON),
    attestationObject: toB64(made.response.attestationObject),
  };
  if (made.response.getTransports) response.transports = made.response.getTransports();
  return post("/enroll/finish", {
    ceremony,
    credential: { id: made.id, rawId: toB64(made.rawId), type: made.type, response },
  });
}

// Authentication, for a name-first sign-in: the credential's id, rawId,
// type, authenticatorData, clientDataJSON, signature and userHandle.
async function signIn(name) {
  const { ceremony, options } = await post("/sign-in/options", { name });
  const key = options.publicKey;
  key.challenge = fromB64(key.challenge);
  for (const held of key.allowCredentials || []) held.id = fromB64(held.id);
  const got = await navigator.credentials.get({ publicKey: key });
  const r = got.response;
  return post("/sign-in/finish", {
    ceremony,
    credential: {
      id: got.id, rawId: toB64(got.rawId), type: got.type,
      response: {
        authenticatorData: toB64(r.authenticatorData),
        clientDataJSON: toB64(r.clientDataJSON),
        signature: toB64(r.signature),
        userHandle: r.userHandle ? toB64(r.userHandle) : null,
      },
    },
  });
}

// One form per page: the ceremony it runs, what it reads, and what it says.
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

ceremony("enroll", "token", enroll,
  (status) => { status.textContent = "Your passkey is enrolled. Sign in with it to continue."; },
  "Not enrolled: ");
ceremony("sign-in", "person", signIn, (_, answer) => location.assign(answer.next), "Not signed in: ");
