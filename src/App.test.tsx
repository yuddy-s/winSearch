import { render, screen } from "@testing-library/react";
import App from "./App";

describe("App", () => {
  it("renders shell headline", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "WinSearch" })).toBeInTheDocument();
  });
});
