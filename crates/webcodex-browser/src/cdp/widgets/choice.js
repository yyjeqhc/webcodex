try {
    connected();
    if (!Array.isArray(requested) || requested.length < 1 || requested.length > 4) fail("widget_not_supported");
    if (disabled(target) || disabled(host)) fail("control_disabled");
    const path = requested.map(norm);
    const samePath = parts => parts.length === path.length && parts.every((part, index) => norm(part) === path[index]);
    const complete = () => reading().some(value => {
        if (path.length === 1) return norm(value) === path[0];
        // Preserve level boundaries: AB/C is not the same selected path as A/BC.
        if (samePath(norm(value).split(/\s*[/／>›,，、]\s*/))) return true;
        if (path.every(part => !part.includes("-")) && samePath(norm(value).split(/\s*-\s*/))) return true;
        return path.every(part => !part.includes(" ")) && samePath(norm(value).split(" "));
    });
    const editableSearch = target instanceof view.HTMLInputElement && !target.readOnly && !target.list;
    const committedInput = (afterChoice = false) => !editableSearch || afterChoice && attr(target, "aria-expanded") === "false"
        || [attr(target, "aria-valuetext"), attr(target, "data-value"), attr(host, "data-value")].map(norm).includes(path.at(-1));
    if (complete() && committedInput()) return { ok: true };

    // Reliable backing-select and datalist fast paths. Editable combobox search text alone
    // is not proof of a committed choice, so ordinary autocomplete inputs use the popup.
    const selects = [...new Set([...walk(host, node => node instanceof view.HTMLSelectElement, 2),
        ...linkedNodes(target).filter(node => node instanceof view.HTMLSelectElement)])];
    if (path.length === 1 && selects.length === 1 && !selects[0].disabled) {
        const backing = selects[0];
        const options = Array.from(backing.options);
        if (options.length > MAX_OPTIONS) fail("widget_bounds_exceeded");
        let matches = options.filter(node => norm(node.value) === path[0]);
        if (!matches.length) matches = options.filter(node => norm(node.text) === path[0]);
        const option = unique(matches);
        if (option) {
            if (backing.value === option.value && option.selected
                && options.filter(item => item.selected).length === 1) return { ok: true };
            changed = true;
            nativeSet(backing, option.value);
            await tick();
            if (backing.isConnected && backing.value === option.value && option.selected) return { ok: true };
            fail("widget_not_supported");
        }
    }
    if (path.length === 1 && target instanceof view.HTMLInputElement && target.list && !target.readOnly) {
        const options = Array.from(target.list.options);
        if (options.length > MAX_OPTIONS) fail("widget_bounds_exceeded");
        const option = unique(options.filter(node => norm(node.value) === path[0] || norm(node.label) === path[0]));
        if (option) {
            changed = true;
            nativeSet(target, option.value);
            await tick();
            if (target.isConnected && target.value === option.value) return { ok: true };
            fail("widget_not_supported");
        }
    }

    await open();
    let previous = new Map();
    let previousRoots = [];
    const signature = node => JSON.stringify([...labels(node), attr(node, "aria-level"), attr(node, "aria-controls")]);
    let parent = null;
    let last = null;
    for (let level = 0; level < path.length; level++) {
        const discovered = await observe(() => {
            const childLinked = parent ? linkedRoots(parent, false).filter(root =>
                !root.contains(parent) && !previousRoots.some(previousRoot => root === previousRoot || root.contains(previousRoot))) : [];
            const roots = childLinked.length ? outerRoots(childLinked) : popupRoots();
            if (!roots.length) return null;
            const options = optionsIn(roots);
            // Prefer newly appeared child options after a cascader step. A duplicate in
            // a different, earlier column must not defeat the exact current-level match.
            const available = !level || childLinked.length ? options.filter(node => node !== parent)
                : options.filter(node => !previous.has(node) || previous.get(node) !== signature(node));
            let matches = available.filter(node => [attr(node, "data-value"), attr(node, "value")].map(norm).includes(path[level]));
            if (!matches.length) matches = available.filter(node => labels(node).includes(path[level]));
            const candidate = unique(matches);
            return candidate ? { candidate, options, roots } : null;
        });
        if (!discovered) fail("option_not_found");
        const { candidate, options, roots } = discovered;
        previous = new Map(options.map(node => [node, signature(node)]));
        previousRoots = roots;
        last = candidate;
        if (!selected(candidate) || level < path.length - 1) {
            // Selection may mutate application state even if a later level fails.
            changed = true;
            activate(candidate);
            await tick();
        }
        parent = candidate;
    }
    const verified = await observe(() => {
        if (complete() && committedInput(true)) return true;
        // Some cascaders display only the leaf. The exact selected path was traversed
        // above, so the leaf readback is sufficient after those known intermediate steps.
        return path.length > 1 && reading().includes(path.at(-1))
            || last?.isConnected && selected(last);
    });
    if (!verified) fail("widget_not_supported");
    return { ok: true };
} catch (error) {
    return await rejected(error);
}
