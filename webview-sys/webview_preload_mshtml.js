(function () {

    var ie8 = window.addEventListener == null;
    var leftButtonCode = ie8 ? 1 : 0;

    function on(target, eventName, handlerReturningBool) {
        var handler = function (e) {
            if (handlerReturningBool(e) === false) {
                if (e.preventDefault) {
                    e.preventDefault();
                } else {
                    e.returnValue = false;
                }
            }
        }
        if (!ie8) {
            target.addEventListener(eventName, handler);
        } else {
            target.attachEvent('on' + eventName, handler);
        }
    }

    on(document, 'mousedown', function (e) {
        if (e.button !== leftButtonCode) return;
        var currentElement = e.target || e.srcElement;
        while (currentElement != null) {
            if (currentElement.hasAttribute('data-webview-no-drag')) {
                break;
            } else if (currentElement.hasAttribute('data-webview-drag')) {
                window.external.drag('');
                break;
            }
            currentElement = currentElement.parentElement;
        }
    });
})();