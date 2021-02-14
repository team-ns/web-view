window.external = {
    invoke: function (x) {
        window.webkit.messageHandlers.external.postMessage(x);
    }
}

window.addEventListener('mousedown', function (e) {
    if (e.button !== 0) return;

    var currentElement = e.target;
    while (currentElement != null) {
        if (currentElement.hasAttribute('data-webview-no-drag')) {
            break;
        } else if (currentElement.hasAttribute('data-webview-drag')) {
            window.webkit.messageHandlers.windowDrag.postMessage(null);
        }
        currentElement = currentElement.parentElement;
    }
});