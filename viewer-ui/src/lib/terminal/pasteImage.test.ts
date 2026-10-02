import { describe, expect, it } from "vitest";
import { pastedImage, pathInput } from "./pasteImage";

function transfer(files: File[], text = ""): DataTransfer {
  return {
    files: files as unknown as FileList,
    getData: (kind: string) => (kind === "text/plain" ? text : ""),
  } as unknown as DataTransfer;
}

const png = new File([new Uint8Array([1])], "shot.png", { type: "image/png" });

describe("pastedImage", () => {
  it("an_image_on_the_clipboard_is_the_paste", () => {
    expect(pastedImage(transfer([png]))?.name).toBe("shot.png");
  });

  it("a_clipboard_carrying_text_as_well_is_a_text_paste", () => {
    expect(pastedImage(transfer([png], "some words"))).toBeNull();
  });

  it("a_file_that_is_not_an_accepted_image_is_not_a_paste", () => {
    const pdf = new File([new Uint8Array([1])], "a.pdf", { type: "application/pdf" });
    expect(pastedImage(transfer([pdf]))).toBeNull();
  });

  it("nothing_on_the_clipboard_is_not_a_paste", () => {
    expect(pastedImage(transfer([]))).toBeNull();
    expect(pastedImage(null)).toBeNull();
  });
});

describe("pathInput", () => {
  it("a_plain_path_is_typed_bare_with_a_separating_space", () => {
    expect(pathInput("/home/me/.nightcrow/tmp/paste-ab.png")).toBe(
      "/home/me/.nightcrow/tmp/paste-ab.png ",
    );
  });

  it("a_path_with_a_space_is_quoted_so_it_stays_one_argument", () => {
    expect(pathInput("C:/Users/A B/.nightcrow/tmp/paste-ab.png")).toBe(
      '"C:/Users/A B/.nightcrow/tmp/paste-ab.png" ',
    );
  });

  it("a_quote_inside_the_path_is_escaped_rather_than_closing_the_quoting", () => {
    expect(pathInput('/tmp/a"b/paste.png')).toBe('"/tmp/a\\"b/paste.png" ');
  });
});
