function copyRecording(element) {
  var copyText = element.getAttribute("data-recording");
  if (copyText) {
    navigator.clipboard.writeText(copyText)
      .then(() => {
        showNotification("Copied to clipboard");
      })
      .catch(() => {
        showNotification("Failed to copy", true);
      });
  } else {
    showNotification("No recording found", true);
  }
}

function showNotification(message, isError = false) {
  let notification = document.createElement('div');
  notification.className = 'recordingNotification';
  notification.textContent = message;
  if (isError) {
    notification.style.borderColor = 'red';
    notification.style.color = 'red';
  }

  document.body.appendChild(notification);

  notification.classList.add('show');

  setTimeout(() => {
    notification.classList.remove('show');
    setTimeout(() => {
      notification.remove();
    }, 400);
  }, 2000);
}
