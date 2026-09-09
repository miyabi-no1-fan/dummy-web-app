// This file is AI generated

declare const math: {
    evaluate: (expr: string) => unknown;
};

const dropZone = document.getElementById('drop-zone') as HTMLDivElement;
const fileInput = document.getElementById('file-input') as HTMLInputElement;
const submitBtn = document.getElementById('submit-btn') as HTMLButtonElement;
const statusEl = document.getElementById('status') as HTMLDivElement;
const fileNameEl = document.getElementById('file-name') as HTMLSpanElement;
const fileSizeEl = document.getElementById('file-size') as HTMLSpanElement;
const resultImg = document.getElementById('result-image') as HTMLImageElement;

const imageDimensionsEl = document.getElementById('image-dimensions') as HTMLDivElement;

const viewer = document.getElementById('viewer') as HTMLDivElement;
const applyBtn = document.getElementById('apply-btn') as HTMLButtonElement;
const transformStatus = document.getElementById('transform-status') as HTMLDivElement;
const newFileBtn = document.getElementById('new-file-btn') as HTMLButtonElement;
const addMatrixBtn = document.getElementById('add-matrix-btn') as HTMLButtonElement;
const matricesContainer = document.getElementById('matrices-container') as HTMLDivElement;

// Only store the original selected file
let selectedFile: File | null = null;

type Matrix = number[];

// Server response body layout: [u32 LE width][u32 LE height][image bytes]
const DIMENSION_HEADER_BYTES = 8;

interface TransformResult {
    blob: Blob;
    width: number;
    height: number;
}

const formatSize = (bytes: number): string => {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
};

const setFile = (file: File | null): void => {
    selectedFile = file;
    if (file) {
        fileNameEl.textContent = file.name;
        fileSizeEl.textContent = formatSize(file.size);
        dropZone.classList.add('has-file');
        submitBtn.disabled = false;
    } else {
        dropZone.classList.remove('has-file');
        submitBtn.disabled = true;
    }
    statusEl.textContent = '';
    statusEl.className = '';
};

fileInput.addEventListener('change', () => setFile(fileInput.files?.[0] ?? null));

['dragenter', 'dragover'].forEach(evt =>
    dropZone.addEventListener(evt, (e: Event) => {
        e.preventDefault();
        dropZone.classList.add('dragging');
    })
);

['dragleave', 'drop'].forEach(evt =>
    dropZone.addEventListener(evt, (e: Event) => {
        e.preventDefault();
        dropZone.classList.remove('dragging');
        if (evt === 'drop') {
            const dragEvent = e as DragEvent;
            const file = dragEvent.dataTransfer?.files[0];
            if (file && dragEvent.dataTransfer) {
                fileInput.files = dragEvent.dataTransfer.files;
                setFile(file);
            }
        }
    })
);

const showViewer = (blob: Blob, width: number, height: number): void => {
    resultImg.src = URL.createObjectURL(blob);
    setDimensionsLabel(width, height);
    viewer.classList.add('visible');
    (document.querySelector('main') as HTMLElement).style.display = 'none';
};

const setDimensionsLabel = (width: number, height: number): void => {
    imageDimensionsEl.textContent = `${width} \u00D7 ${height}`;
};

const packMatrixHeader = (matrices: Matrix[]): Uint8Array => {
    const count = matrices.length;
    // 1 byte for count + 16 bytes (4 floats * 4 bytes) per matrix
    const buf = new ArrayBuffer(1 + count * 16);
    const view = new DataView(buf);

    view.setUint8(0, count);

    matrices.forEach((matrix, i) => {
        matrix.forEach((val, j) => {
            // Offset: 1 byte for count + (16 bytes per previous matrix) + (4 bytes per previous float)
            view.setFloat32(1 + (i * 16) + (j * 4), val, true);
        });
    });

    return new Uint8Array(buf);
};

const sendImageTransform = async (dataBlob: Blob, matrices: Matrix[]): Promise<TransformResult> => {
    const header = packMatrixHeader(matrices);
    const fileBuf = await dataBlob.arrayBuffer();
    const fileBytes = new Uint8Array(fileBuf);

    const payload = new Uint8Array(header.length + fileBytes.length);
    payload.set(header, 0);
    payload.set(fileBytes, header.length);

    const res = await fetch('/submit', {
        method: 'POST',
        headers: { 'Content-Type': dataBlob.type || 'application/octet-stream' },
        body: payload,
    });

    if (!res.ok) throw new Error(String(res.status));

    const responseBuf = await res.arrayBuffer();
    if (responseBuf.byteLength < DIMENSION_HEADER_BYTES) {
        throw new Error('malformed response');
    }

    const view = new DataView(responseBuf);
    const width = view.getUint32(0, true);
    const height = view.getUint32(4, true);

    const imageBytes = new Uint8Array(responseBuf, DIMENSION_HEADER_BYTES);
    const contentType = res.headers.get('X-Image-Content-Type') || dataBlob.type || 'application/octet-stream';
    const blob = new Blob([imageBytes], { type: contentType });

    return { blob, width, height };
};

// Use math.js to evaluate the expression
const parseExpr = (expr: string): number => {
    if (!expr || !expr.trim()) return 0;
    try {
        // math.evaluate handles functions like sin, cos, ln, pi natively
        const result = math.evaluate(expr);
        return typeof result === 'number' ? result : NaN;
    } catch (e) {
        return NaN;
    }
};

submitBtn.addEventListener('click', async () => {
    if (!selectedFile) return;

    submitBtn.disabled = true;
    statusEl.textContent = 'Uploading...';
    statusEl.className = '';

    try {
        const { blob, width, height } = await sendImageTransform(selectedFile, [[1.0, 0.0, 0.0, 1.0]]);
        showViewer(blob, width, height);
        statusEl.textContent = 'File submitted.';
        statusEl.className = 'success';
        fileInput.value = '';
        // We do not clear selectedFile here so we can reuse it later
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const isNetwork = !Number(message);
        statusEl.textContent = isNetwork ? 'Network error. Check your connection and try again.' : `Submit failed (${message}). Try again.`;
        statusEl.className = 'error';
        submitBtn.disabled = false;
    }
});

applyBtn.addEventListener('click', async () => {
    if (!selectedFile) return;

    const matrices: Matrix[] = [];
    let hasError = false;

    const grids = document.querySelectorAll<HTMLDivElement>('.matrix-grid');
    grids.forEach(grid => {
        const inputs = Array.from(grid.querySelectorAll<HTMLInputElement>('input'));

        const values = inputs.map(input => parseExpr(input.value));

        if (values.some(v => isNaN(v))) hasError = true;
        matrices.push(values);
    });

    if (hasError || matrices.length === 0) {
        transformStatus.textContent = 'All matrix fields must be valid numbers or expressions (e.g. "sin(pi/2)").';
        transformStatus.className = 'error';
        return;
    }

    if (matrices.length > 255) {
        transformStatus.textContent = 'Maximum of 255 matrices allowed.';
        transformStatus.className = 'error';
        return;
    }

    applyBtn.disabled = true;
    transformStatus.textContent = '';
    transformStatus.className = '';

    try {
        const { blob, width, height } = await sendImageTransform(selectedFile, matrices);
        resultImg.src = URL.createObjectURL(blob);
        setDimensionsLabel(width, height);
    } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        const isNetwork = !Number(message);
        transformStatus.textContent = isNetwork ? 'Network error. Try again.' : `Transform failed (${message}).`;
        transformStatus.className = 'error';
    } finally {
        applyBtn.disabled = false;
    }
});

const createMatrixUI = (): DocumentFragment => {
    const template = document.getElementById('matrix-template') as HTMLTemplateElement;
    const clone = template.content.cloneNode(true) as DocumentFragment;

    // We need a reference to the top-level element in the template to remove it later
    const row = clone.querySelector('.expr-row') as HTMLElement;

    const removeBtn = clone.querySelector('.remove-matrix-btn') as HTMLButtonElement;
    removeBtn.addEventListener('click', () => {
        // Prevent removing the very last matrix
        if (matricesContainer.children.length > 1) {
            row.remove();
        }
    });

    return clone;
};

addMatrixBtn.addEventListener('click', () => {
    matricesContainer.appendChild(createMatrixUI());
});

newFileBtn.addEventListener('click', () => {
    viewer.classList.remove('visible');
    (document.querySelector('main') as HTMLElement).style.display = '';

    setFile(null); // Clear the selected file on return

    transformStatus.textContent = '';
    transformStatus.className = '';

    // Reset to a single identity matrix
    matricesContainer.innerHTML = '';
    matricesContainer.appendChild(createMatrixUI());
});

// Initialize the first matrix on page load
matricesContainer.appendChild(createMatrixUI());