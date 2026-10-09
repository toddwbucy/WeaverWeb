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

const form = document.getElementById("enroll");
if (form) {
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const status = document.getElementById("status");
    status.textContent = "Waiting for your authenticator.";
    try {
      await enroll(form.elements.token.value.trim());
      form.reset();
      status.textContent = "Your passkey is enrolled. Sign in with it to continue.";
    } catch (error) {
      status.textContent = "Not enrolled: " + error.message;
    }
  });
}
