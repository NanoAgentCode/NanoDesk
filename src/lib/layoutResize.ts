export function beginResize(onMove: (event: MouseEvent) => void, cursor = "col-resize") {
  const previousCursor = document.body.style.cursor;
  document.body.style.cursor = cursor;
  document.body.classList.add("is-resizing");

  const handleMove = (event: MouseEvent) => {
    event.preventDefault();
    onMove(event);
  };
  const handleUp = () => {
    document.body.style.cursor = previousCursor;
    document.body.classList.remove("is-resizing");
    window.removeEventListener("mousemove", handleMove);
    window.removeEventListener("mouseup", handleUp);
  };

  window.addEventListener("mousemove", handleMove);
  window.addEventListener("mouseup", handleUp);
}
