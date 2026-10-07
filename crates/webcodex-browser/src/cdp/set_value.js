function(requested) {
            if (!(this instanceof HTMLInputElement) && !(this instanceof HTMLTextAreaElement)) {
                return { ok: false, kind: "element_not_value_control" };
            }
            if (this.disabled || this.readOnly) {
                return { ok: false, kind: "control_disabled" };
            }
            const textArea = this instanceof HTMLTextAreaElement;
            const type = (this.type || "text").toLowerCase();
            const textTypes = new Set(["text", "search", "email", "tel", "url", "password"]);
            if (textArea || textTypes.has(type)) {
                // Use the native setter so framework value trackers see the input event.
                // This is replacement, unlike Input.insertText's caret insertion.
                const prototype = textArea ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
                const setter = Object.getOwnPropertyDescriptor(prototype, "value").set;
                setter.call(this, requested);
                this.dispatchEvent(new Event("input", { bubbles: true }));
                this.dispatchEvent(new Event("change", { bubbles: true }));
                return this.value === requested ? { ok: true }
                    : { ok: false, kind: "value_postcondition_failed", mutated: true };
            }
            const structuredTypes = new Set([
                "date", "datetime-local", "month", "week", "time", "number", "range", "color"
            ]);
            if (!structuredTypes.has(type)) {
                return { ok: false, kind: "unsupported_value_control" };
            }
            const probe = document.createElement("input");
            probe.type = type;
            for (const attribute of ["min", "max", "step"]) {
                if (this.hasAttribute(attribute)) {
                    probe.setAttribute(attribute, this.getAttribute(attribute));
                }
            }
            probe.value = requested;
            if (probe.value !== requested || !probe.checkValidity()) {
                return { ok: false, kind: "invalid_control_value" };
            }
            this.value = requested;
            if (this.value !== requested) {
                return { ok: false, kind: "value_postcondition_failed", mutated: true };
            }
            this.dispatchEvent(new Event("input", { bubbles: true }));
            this.dispatchEvent(new Event("change", { bubbles: true }));
            if (this.value !== requested) {
                return { ok: false, kind: "value_postcondition_failed", mutated: true };
            }
            return { ok: true };
        }
