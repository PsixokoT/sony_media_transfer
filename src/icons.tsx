/** Segoe Fluent Icons, built into Windows 11: https://learn.microsoft.com/windows/apps/design/style/segoe-fluent-icons-font */
const icons = {
    refresh: '',
    eject: '',
    copy: '',
    settings: '',
    choose: '', // Folder
    openFolder: '', // FolderOpen
    openExternal: '', // OpenInNewWindow
    close: '', // Cancel
    hide: '', // Hide
    unhide: '', // View
    prev: '', // ChevronLeft
    next: '', // ChevronRight
};

export const Icon = (props: { name: keyof typeof icons }) => (
    <span className="icon" aria-hidden>
        {icons[props.name]}
    </span>
);
