window.external = {
    invoke: function (string) {
        window.chrome.webview.postMessage('m' + string);
    }
}

window.addEventListener('mousedown', function(e) {
    if (e.button !== 0) return;

    var currentElement = e.target;
    while (currentElement != null) {
        if (currentElement.hasAttribute('data-webview-no-drag')) {
            break;
        }
        else if (currentElement.hasAttribute('data-webview-drag')) {
            window.setImmediate(function() {
                window.chrome.webview.postMessage('d')
            });
            break;
        }
        currentElement = currentElement.parentElement;
    }
});