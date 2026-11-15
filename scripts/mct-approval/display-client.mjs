const id = document.body.dataset.display;
const revision = Number(document.body.dataset.revision || 0);
if (id) {
  const events = new EventSource(`/d/${id}/events`);
  events.addEventListener('status', event => {
    const update = JSON.parse(event.data);
    if (update.revision !== revision) location.reload();
  });
}
