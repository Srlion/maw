// Simple counter with button
let count = 0;
const btn = document.createElement('button');
btn.textContent = 'Click me!';
btn.onclick = () => { count++; btn.textContent = `Clicks: ${count}`; };
document.body.appendChild(btn);
